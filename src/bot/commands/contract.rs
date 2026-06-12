use crate::bot::auth::{level_of, AccessLevel};
use crate::bot::data::{Context, Data, Error};
use crate::database::structures::Status;
use crate::database::Database;
use chrono::{DateTime, NaiveDateTime, Utc};
use mongodb::bson;
use mongodb::bson::{doc, Document};
use poise::CreateReply;
use rand::distr::Alphanumeric;
use rand::Rng;
use serenity::all::{
    ButtonStyle, ComponentInteraction, CreateActionRow, CreateButton, CreateEmbed,
    CreateEmbedFooter, CreateInteractionResponse, CreateInteractionResponseMessage, ReactionType,
    UserId,
};
use serenity::utils::MessageBuilder;

pub(crate) const PAGE_SIZE: u64 = 10;

/// Pagination state of a `/contract list` message
pub struct ListMessageInfo {
    user_id: UserId,
    filter: Option<Document>,
    page: u64,
}

#[derive(poise::ChoiceParameter)]
pub enum StatusFilter {
    #[name = "active"]
    Active,
    #[name = "pending"]
    Pending,
    #[name = "ended"]
    Ended,
    #[name = "all"]
    All,
}

/// Manage contracts
#[poise::command(slash_command, subcommands("start", "end", "list"))]
pub async fn contract(_ctx: Context<'_>) -> Result<(), Error> {
    // Parent command of subcommands, never invoked directly.
    Ok(())
}

/// Returns false (and replies) when the invoking user is not an admin.
async fn ensure_admin(ctx: &Context<'_>) -> Result<bool, Error> {
    if level_of(ctx) >= AccessLevel::Admin {
        return Ok(true);
    }

    log::warn!("Unauthorized user: {}", ctx.author().id);
    log::warn!("Secret admins: {:?}", ctx.data().secrets.admins);

    ctx.send(
        CreateReply::default()
            .content("You are not authorized to use this command.")
            .ephemeral(true),
    )
    .await?;

    Ok(false)
}

/// Create a new contract
#[poise::command(slash_command)]
pub async fn start(
    ctx: Context<'_>,
    #[description = "The name of the contract"] contract_name: String,
    #[description = "The ID of the faction for the contract"] faction_id: u64,
    #[description = "The minimum chance of success to count for payment"] min_chance: u64,
    #[description = "The cut the faction gets from the contract (default 10%)"] faction_cut: Option<u64>,
    #[description = "Optional contract start time in UTC as YYYY-MM-DD HH:MM"] start_time: Option<String>,
) -> Result<(), Error> {
    if !ensure_admin(&ctx).await? {
        return Ok(());
    }

    let faction_cut = faction_cut.unwrap_or(10);

    let started_at = match start_time {
        Some(start_time) => match parse_contract_start_time(&start_time) {
            Ok(started_at) => started_at,
            Err(error) => {
                ctx.send(CreateReply::default().content(error).ephemeral(true))
                    .await?;
                return Ok(());
            }
        },
        None => Utc::now(),
    };

    let status = if started_at > Utc::now() {
        Status::Pending
    } else {
        Status::Active
    };

    let faction_data = ctx
        .data()
        .torn_api
        .lock()
        .await
        .get_faction_data(faction_id)
        .await
        .unwrap();

    if let Some(error) = faction_data.get("error") {
        log::info!("Error: {:?}", error);
        ctx.send(
            CreateReply::default()
                .content("Invalid faction ID")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    log::info!(
        "Processing create subcommand with contract_name: {} and faction_id: {}",
        contract_name,
        faction_id
    );

    let contract = crate::database::structures::Contract {
        id: None,
        contract_id: generate_contract_id().await,
        contract_name,
        faction_id,
        min_chance,
        started: started_at.timestamp() as u64,
        ended: 0,
        status,
        faction_cut: faction_cut as i64,
        revives_synced: false,
    };

    let status_label = match contract.status {
        Status::Active => "active",
        Status::Pending => "pending",
        Status::Ended => "ended",
    };

    let message = MessageBuilder::new()
        .push("Contract created with ID: ")
        .push_mono(contract.contract_id.clone())
        .push(" at ")
        .push(format!("<t:{}:f>", contract.started.clone()))
        .push(" and is ")
        .push(status_label)
        .push(".")
        .build();

    Database::insert(contract).await.unwrap();

    ctx.send(CreateReply::default().content(message).ephemeral(true))
        .await?;

    Ok(())
}

/// End Contract
#[poise::command(slash_command)]
pub async fn end(
    ctx: Context<'_>,
    #[description = "ID of the contract to end"] contract_id: String,
) -> Result<(), Error> {
    if !ensure_admin(&ctx).await? {
        return Ok(());
    }

    log::info!("Processing end subcommand with contract_id: {}", contract_id);

    let result: Vec<crate::database::structures::Contract> =
        Database::get_collection_with_filter(Some(doc! {"contract_id": contract_id.clone()}))
            .await
            .unwrap();

    let mut message = MessageBuilder::new()
        .push("No contract found with ID: ")
        .push_mono(contract_id.clone())
        .build();

    if result.is_empty() {
        log::warn!("No contract found with ID: {}", contract_id);
        ctx.send(CreateReply::default().content(message).ephemeral(true))
            .await?;
        return Ok(());
    }

    let mut contract = result[0].clone();

    if contract.status == Status::Ended {
        message = MessageBuilder::new()
            .push("This contract has already ended.")
            .build()
    } else {
        contract.status = Status::Ended;
        contract.ended = Utc::now().timestamp() as u64;

        Database::update(contract.clone(), doc! {"contract_id": contract_id.clone()})
            .await
            .unwrap();

        message = MessageBuilder::new()
            .push(format!(
                "Contract {} ({}) ended at {}",
                contract.contract_name,
                contract.contract_id,
                format_time(contract.ended)
            ))
            .build();
    }

    ctx.send(CreateReply::default().content(message).ephemeral(true))
        .await?;

    Ok(())
}

/// List contracts
#[poise::command(slash_command)]
pub async fn list(
    ctx: Context<'_>,
    #[description = "Choose what contracts to list"] status: StatusFilter,
) -> Result<(), Error> {
    if !ensure_admin(&ctx).await? {
        return Ok(());
    }

    log::info!("Processing list subcommand");

    let filter = match status {
        StatusFilter::Active => Some(doc! {"status": bson::to_bson(&Status::Active).unwrap()}),
        StatusFilter::Pending => Some(doc! {"status": bson::to_bson(&Status::Pending).unwrap()}),
        StatusFilter::Ended => Some(doc! {"status": bson::to_bson(&Status::Ended).unwrap()}),
        StatusFilter::All => None,
    };

    let (content, embed, components) = create_page(1, PAGE_SIZE, filter.clone()).await;

    let handle = ctx
        .send(
            CreateReply::default()
                .content(content)
                .embed(embed)
                .components(components),
        )
        .await?;

    let message = handle.message().await?;

    ctx.data().contract_pages.lock().await.insert(
        message.id,
        ListMessageInfo {
            user_id: ctx.author().id,
            filter,
            page: 1,
        },
    );

    Ok(())
}

/// Handles the next/previous pagination buttons on `/contract list` messages
pub async fn handle_pagination(
    ctx: &serenity::all::Context,
    data: &Data,
    component: &ComponentInteraction,
) -> Result<(), Error> {
    log::info!("Processing button interaction");

    let (page, filter) = {
        let mut pages = data.contract_pages.lock().await;

        let Some(info) = pages.get_mut(&component.message.id) else {
            // Unknown message (e.g. the bot restarted since the list was created)
            component.defer(&ctx.http).await?;
            return Ok(());
        };

        if component.user.id != info.user_id {
            // Only original author can interact with the buttons on that specific message
            component.defer(&ctx.http).await?;
            return Ok(());
        }

        if component.data.custom_id == "next" {
            info.page += 1;
        } else if component.data.custom_id == "previous" {
            info.page -= 1;
        }

        (info.page, info.filter.clone())
    };

    let (content, embed, components) = create_page(page, PAGE_SIZE, filter).await;

    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .embed(embed)
                    .components(components),
            ),
        )
        .await?;

    Ok(())
}

async fn create_page(
    page: u64,
    page_size: u64,
    filter: Option<Document>,
) -> (String, CreateEmbed, Vec<CreateActionRow>) {
    promote_pending_contracts().await;

    let size = Database::get_collection_size(filter.clone()).await.unwrap();

    let options = mongodb::options::FindOptions::builder()
        .skip((page - 1) * page_size)
        .limit(page_size as i64)
        .sort(doc! {"_id": -1})
        .build();

    let contracts: Vec<crate::database::structures::Contract> =
        Database::get_collection_with_filter_and_options(filter, Some(options))
            .await
            .unwrap();

    let pages = size.div_ceil(page_size);

    let mut table = String::from("```\n");
    table.push_str(&format!(
        "{:<25} {:<8} {:<8} {:<12}\n",
        "Name", "Status", "ID", "Started"
    ));
    table.push_str(&format!(
        "{:-<25} {:-<8} {:-<8} {:-<12}\n",
        "", "", "", ""
    ));

    for contract in &contracts {
        let name = if contract.contract_name.len() > 24 {
            format!("{}…", &contract.contract_name[..23])
        } else {
            contract.contract_name.clone()
        };
        let status = match contract.status {
            Status::Active => "Active",
            Status::Pending => "Pending",
            Status::Ended => "Ended",
        };
        let started = chrono::DateTime::from_timestamp(contract.started as i64, 0)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        table.push_str(&format!(
            "{:<25} {:<8} {:<8} {:<12}\n",
            name, status, contract.contract_id, started
        ));
    }
    table.push_str("```");

    let embed = CreateEmbed::new()
        .title("Contracts")
        .description(table)
        .timestamp(Utc::now())
        .footer(CreateEmbedFooter::new(format!(
            "Page {} of {}",
            page, pages
        )));

    let mut buttons = Vec::new();

    if page > 1 {
        buttons.push(
            CreateButton::new("previous")
                .style(ButtonStyle::Primary)
                .emoji(ReactionType::Unicode("⬅️".to_string())),
        );
    }

    if page < pages && pages > 1 {
        buttons.push(
            CreateButton::new("next")
                .style(ButtonStyle::Primary)
                .emoji(ReactionType::Unicode("➡️".to_string())),
        );
    }

    let components = if buttons.is_empty() {
        vec![]
    } else {
        vec![CreateActionRow::Buttons(buttons)]
    };

    ("List of contracts".to_string(), embed, components)
}

async fn promote_pending_contracts() {
    let pending_contracts = Database::get_collection_with_filter::<crate::database::structures::Contract>(Some(
        doc! {"status": bson::to_bson(&Status::Pending).unwrap()}
    ))
    .await
    .unwrap();

    let now = Utc::now().timestamp() as u64;

    for mut contract in pending_contracts {
        if contract.started <= now {
            contract.status = Status::Active;
            Database::update(contract.clone(), doc! {"contract_id": contract.contract_id.clone()})
                .await
                .unwrap();
        }
    }
}

fn parse_contract_start_time(start_time: &str) -> Result<DateTime<Utc>, String> {
    let parsed = NaiveDateTime::parse_from_str(start_time, "%Y-%m-%d %H:%M").map_err(|_| {
        "Invalid start time format. Use YYYY-MM-DD HH:MM in UTC.".to_string()
    })?;

    Ok(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc))
}

fn format_time(time: u64) -> String {
    format!("<t:{}:f>", time)
}

async fn generate_contract_id() -> String {
    loop {
        // Generate a 6-character alphanumeric string
        let contract_id: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(6) // Adjust the length as needed
            .map(|c| c as char)
            .collect();

        // Check if the generated ID is unique
        let result: Vec<crate::database::structures::Contract> =
            Database::get_collection_with_filter(Some(doc! {"contract_id": contract_id.clone()}))
                .await
                .unwrap();

        if result.is_empty() {
            return contract_id;
        }
    }
}

use crate::bot::auth::{level_of, AccessLevel};
use crate::bot::data::{Context, Error};
use crate::bot::tools::get_player_cache::get_player_cache;
use crate::database::structures::{Contract, ReviveEntry, Status};
use crate::database::Database;
use mongodb::bson::{doc, Bson};
use poise::CreateReply;
use serenity::builder::{CreateEmbed, CreateMessage};
use std::collections::HashMap;

/// Generate contract report
#[poise::command(slash_command)]
pub async fn report(
    ctx: Context<'_>,
    #[description = "The contract ID of the player you want to report"] contract_id: String,
) -> Result<(), Error> {
    let secrets = &ctx.data().secrets;

    let is_admin = level_of(&ctx) >= AccessLevel::Admin;

    let contract = Database::get_collection_with_filter::<Contract>(Some(doc! {
        "contract_id": contract_id.clone()
    }))
    .await
    .unwrap()
    .pop();

    let Some(mut contract) = contract else {
        ctx.send(
            CreateReply::default()
                .content("Contract not found")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    };

    if contract.status != Status::Ended {
        let msg = match contract.status {
            Status::Pending => format!(
                "Contract hasn't started yet — it starts <t:{}:f>.",
                contract.started
            ),
            _ => "Contract is still active. Live reports will be implemented in the future hopefully."
                .to_string(),
        };
        ctx.send(CreateReply::default().content(msg).ephemeral(true))
            .await?;
        return Ok(());
    }

    let syncing_status = if !contract.revives_synced {
        ctx.defer().await?;
        let status = ctx
            .send(
                CreateReply::default().content("Generating report — syncing revive data…"),
            )
            .await?;

        if let Err(e) = ctx
            .data()
            .revive_monitor
            .sync_for_contract(contract.ended)
            .await
        {
            status
                .edit(
                    ctx,
                    CreateReply::default()
                        .content(format!("Failed to sync revive data: {e:#}")),
                )
                .await?;
            return Ok(());
        }

        contract.revives_synced = true;
        Database::update(contract.clone(), doc! {"contract_id": contract_id.clone()})
            .await
            .unwrap();

        Some(status)
    } else {
        None
    };

    let reviving_faction_ids = secrets.reviving_faction_ids();
    let reviver_faction_filter: Vec<Bson> = reviving_faction_ids
        .iter()
        .map(|id| Bson::Int64(*id as i64))
        .collect();

    let revives = Database::get_collection_with_filter::<ReviveEntry>(Some(doc! {
        "timestamp": {
            "$gte": Bson::Int64(contract.started as i64),
            "$lte": Bson::Int64(contract.ended as i64)
        },
        "target_faction": Bson::Int64(contract.faction_id as i64),
        "reviver_faction": { "$in": reviver_faction_filter }
    }))
    .await
    .unwrap();

    let mut per_player: HashMap<u64, Vec<ReviveEntry>> = HashMap::new();
    let mut successful = 0;
    let mut failed = 0;
    let len = revives.len();

    for revive in revives {
        per_player
            .entry(revive.reviver_id)
            .or_insert(Vec::new())
            .push(revive.clone());

        if revive.result == "success" {
            successful += 1;
        } else if revive.chance > contract.min_chance as f32 {
            failed += 1;
        }
    }

    let api = ctx.data().torn_api.clone();

    let faction_data_target = api
        .lock()
        .await
        .get_faction_data(contract.faction_id)
        .await
        .unwrap();

    let mut reviver_faction_labels = Vec::new();
    for id in &reviving_faction_ids {
        let faction_data = api.lock().await.get_faction_data(*id).await.unwrap();
        reviver_faction_labels.push(format!(
            "{} ({})",
            faction_data["name"].as_str().unwrap(),
            faction_data["ID"].as_u64().unwrap()
        ));
    }

    let reviver_field_name = if reviving_faction_ids.len() == 1 {
        "Reviving Faction"
    } else {
        "Reviving Factions"
    };

    let price = vec![
        format_with_commas((successful * 900000 + failed * 1000000) as u64),
        format_with_commas(
            (successful * 900000 + failed * 1000000) as u64
                * (1.0 + contract.faction_cut as f64 / 100.0) as u64,
        ),
    ];

    let mut embed = CreateEmbed::new()
        .title(contract.contract_name.clone() + " Report")
        .description(" ")
        .field(
            reviver_field_name,
            reviver_faction_labels.join("\n"),
            true,
        )
        .field(
            "Target Faction",
            format!(
                "{} ({})",
                faction_data_target["name"].as_str().unwrap(),
                faction_data_target["ID"].as_u64().unwrap()
            ),
            true,
        )
        .field("", "", false)
        .field("Successful Revives", successful.to_string(), true)
        .field("Failed Counted", (failed).to_string(), true)
        .field(
            "Failed Ignored",
            (len - successful - failed).to_string(),
            true,
        )
        .field("Started", format!("<t:{}:f>", contract.started), true)
        .field("Ended", format!("<t:{}:f>", contract.ended), true)
        .field("", "", false)
        .field(
            "Final Price",
            price
                .get(!is_admin as usize)
                .unwrap_or(&"".to_string())
                .to_string(),
            true,
        );

    if is_admin {
        embed = embed.field(
            format!("Final Price (+{}%)", contract.faction_cut),
            format!(
                "${}",
                format_with_commas(
                    ((successful * 900000 + failed * 1000000) as f64
                        * (1.0 + contract.faction_cut as f64 / 100.0)) as u64
                )
            ),
            true,
        );
    };

    if let Some(status) = syncing_status {
        status.delete(ctx).await?;
    }

    ctx.send(CreateReply::default().embed(embed)).await?;

    // List of rewards is only for admins
    if !is_admin {
        return Ok(());
    }

    let mut reward_list = Vec::new();

    for id in per_player.keys() {
        // I could get the name, but latter when I talk it over I will
        // probably also need the revive skill, now if the report has many players involved
        // and it hits the rate limit, it will be a real problem as the API will freeze,
        // here any everywhere else.
        // TODO : I will probably need some type of cashing system for the skill the will be updated based on revive_monitor.
        let player_data = match get_player_cache(*id, &mut *api.lock().await).await {
            Some(player) => player,
            None => continue,
        };

        let player_name = player_data.name.clone();

        let success = per_player[id]
            .iter()
            .filter(|r| r.result == "success")
            .count();

        let failed_counted = per_player[id]
            .iter()
            .filter(|r| r.chance >= contract.min_chance as f32 && r.result == "failure")
            .count();

        let _failed = per_player[id].len() - (failed_counted + success);

        reward_list.push((
            (success * 900000 + failed_counted * 1000000) as u64, // Monetary value for sorting
            format!(
                "* **{} [{}]** - ${} (s: {}, f: {})",
                player_name,
                id,
                format_with_commas((success * 900000 + failed_counted * 1000000) as u64),
                success,
                failed_counted
            ),
        ));
    }

    reward_list.sort_by(|a, b| b.0.cmp(&a.0));

    let pages = reward_list.chunks(10).collect::<Vec<_>>();

    for (i, page) in pages.iter().enumerate() {
        let embed = CreateEmbed::new()
            .title(format!("Rewards ({}/{})", i + 1, pages.len()))
            .description(
                page.iter()
                    .map(|(_, s)| s.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            );

        ctx.channel_id()
            .send_message(ctx.serenity_context(), CreateMessage::new().embed(embed))
            .await?;
    }

    Ok(())
}

fn format_with_commas(number: u64) -> String {
    let mut chars: Vec<_> = number.to_string().chars().collect();
    let len = chars.len();
    for i in (1..len).rev() {
        if (len - i) % 3 == 0 {
            chars.insert(i, ',');
        }
    }
    chars.into_iter().collect()
}

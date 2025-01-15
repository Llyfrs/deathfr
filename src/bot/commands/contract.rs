use crate::bot::commands::command::Commands;
use crate::bot::Secrets;
use crate::database::structures::Status;
use crate::database::Database;
use crate::torn_api;
use crate::torn_api::{request_update, TornAPI};
use chrono::Utc;
use log::log;
use mongodb::bson;
use mongodb::bson::{doc, Document};
use rand::distributions::Alphanumeric;
use rand::Rng;
use serenity::all::CommandDataOptionValue::SubCommand;
use serenity::all::{ButtonStyle, CommandDataOption, CommandDataOptionValue, CommandInteraction, CommandOptionType, Context, CreateButton, CreateCommand, CreateEmbed, CreateEmbedFooter, CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage, EditInteractionResponse, EmbedField, Interaction, Message, MessageId, Permissions, ReactionType, UserId};
use serenity::async_trait;
use serenity::builder::CreateCommandOption;
use serenity::utils::MessageBuilder;
use std::cmp::PartialEq;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

const PAGE_SIZE: u64 = 5;

pub struct Contract {
    api: Arc<Mutex<TornAPI>>,
    secrets: Secrets,
    responses: HashMap<MessageId, ListMessageInfo>,
}

struct ListMessageInfo {
    user_id: UserId,
    filter: Option<Document>,
    page: u64,
}

impl Contract {
    pub fn new(secrets: Secrets, api: Arc<Mutex<TornAPI>>) -> Self {
        Self {
            api,
            secrets,
            responses: HashMap::new(),
        }
    }
}

#[async_trait]
impl Commands for Contract {
    fn register(&self) -> CreateCommand {
        CreateCommand::new("contract")
            .description("Manage contracts")
            .add_option(
                // Create a new contract
                CreateCommandOption::new(
                    CommandOptionType::SubCommand,
                    "start",
                    "Create a new contract",
                )
                .add_sub_option(
                    CreateCommandOption::new(
                        CommandOptionType::String,
                        "contract_name",
                        "The name of the contract",
                    )
                    .required(true),
                )
                .add_sub_option(
                    CreateCommandOption::new(
                        CommandOptionType::Integer,
                        "faction_id",
                        "The ID of the faction for the contract",
                    )
                    .required(true),
                ).add_sub_option(
                    CreateCommandOption::new(
                        CommandOptionType::Integer,
                        "min_chance",
                        "The minimum chance of success to count for payment",
                    ).required(true),
                ),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "end", "End Contract")
                    .add_sub_option(
                        CreateCommandOption::new(
                            CommandOptionType::String,
                            "contract_id",
                            "ID of the contract to end",
                        )
                        .required(true),
                    ),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::SubCommand, "list", "List contracts")
                    .add_sub_option(
                        CreateCommandOption::new(
                            CommandOptionType::String,
                            "status",
                            "Choose what contracts to list",
                        )
                        .add_string_choice("active", "active")
                        .add_string_choice("ended", "ended")
                        .add_string_choice("all", "all")
                        .required(true),
                    ),
            )


    }

    async fn action(&mut self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(ref command) = interaction {
            if command.data.name.as_str() != "contract" {
                return;
            }

            let sub_command = command.data.options[0].name.as_str();

            if sub_command == "start" {
                if let CommandDataOption {
                    value: SubCommand(sub_options),
                    ..
                } = &command.data.options[0]
                {
                    let contract_name =
                        if let CommandDataOptionValue::String(value) = &sub_options[0].value {
                            value.clone()
                        } else {
                            log::warn!("Missing or invalid 'contract_name' value.");
                            return;
                        };

                    let faction_id =
                        if let CommandDataOptionValue::Integer(value) = &sub_options[1].value {
                            *value as u64
                        } else {
                            log::warn!("Missing or invalid 'faction_id' value.");
                            return;
                        };

                    let min_chance =
                        if let CommandDataOptionValue::Integer(value) = &sub_options[2].value {
                            *value as u64
                        } else {
                            log::warn!("Missing or invalid 'min_chance' value.");
                            return;
                        };

                    let faction_data = self
                        .api
                        .lock()
                        .await
                        .get_faction_data(faction_id)
                        .await
                        .unwrap();

                    if let Some(error) = faction_data.get("error") {
                        log::info!("Error: {:?}", error);
                        create_response(&ctx, command.clone(), "Invalid faction ID".to_string())
                            .await;
                        return;
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
                        faction_id: faction_id as u64,
                        min_chance: min_chance as u64,
                        started: Utc::now().timestamp() as u64,
                        ended: 0,
                        status: Status::Active,
                    };

                    let message = MessageBuilder::new()
                        .push("Contract created with ID: ")
                        .push_mono(contract.contract_id.clone())
                        .push(" at ")
                        .push(format!("<t:{}:f>", contract.started.clone()))
                        .build();

                    Database::insert(contract).await.unwrap();

                    command
                        .create_response(
                            &ctx.http,
                            CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .content(message)
                                    .ephemeral(true),
                            ),
                        )
                        .await
                        .unwrap();
                } else {
                    log::warn!("Invalid options structure for 'create' subcommand.");
                }
            }

            if sub_command == "end" {
                if let CommandDataOption {
                    value: SubCommand(sub_options),
                    ..
                } = &command.data.options[0]
                {
                    let contract_id =
                        if let CommandDataOptionValue::String(value) = &sub_options[0].value {
                            value.clone()
                        } else {
                            log::warn!("Missing or invalid 'contract_id' value.");
                            return;
                        };

                    log::info!(
                        "Processing end subcommand with contract_id: {}",
                        contract_id
                    );

                    let result: Vec<crate::database::structures::Contract> =
                        Database::get_collection_with_filter(Some(
                            doc! {"contract_id": contract_id.clone()},
                        ))
                        .await
                        .unwrap();

                    let mut message = MessageBuilder::new()
                        .push("No contract found with ID: ")
                        .push_mono(contract_id.clone())
                        .build();

                    if result.is_empty() {
                        log::warn!("No contract found with ID: {}", contract_id);
                        create_response(&ctx, command.clone(), message.clone()).await;
                        return;
                    }

                    let mut contract = result[0].clone();

                    if contract.status == Status::Ended {
                        message = MessageBuilder::new()
                            .push("This contract has already ended.")
                            .build()
                    } else {
                        contract.status = Status::Ended;
                        contract.ended = Utc::now().timestamp() as u64;

                        Database::update(
                            contract.clone(),
                            doc! {"contract_id": contract_id.clone()},
                        )
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

                    request_update(); // Request an update to the revived monitor, so when a report is called it can be up to date
                    create_response(&ctx, command.clone(), message.clone()).await;
                } else {
                    log::warn!("Invalid options structure for 'end' subcommand.");
                }
            }

            if sub_command == "list" {
                log::info!("Processing list subcommand");

                if let CommandDataOption {
                    value: SubCommand(sub_options),
                    ..
                } = &command.data.options[0]
                {
                    let status =
                        if let CommandDataOptionValue::String(value) = &sub_options[0].value {
                            value.clone()
                        } else {
                            log::warn!("Missing or invalid 'status' value.");
                            return;
                        };

                    let filter = match status.as_str() {
                        "active" => Some(doc! {"status": bson::to_bson(&Status::Active).unwrap()}),
                        "ended" => Some(doc! {"status": bson::to_bson(&Status::Ended).unwrap()}),
                        "all" => None,
                        _ => {
                            log::warn!("Invalid status value: {}", status);
                            return;
                        }
                    };

                    let embed = create_page_embed(1, PAGE_SIZE, filter.clone()).await;

                    command
                        .create_response(&ctx.http, CreateInteractionResponse::Message(embed))
                        .await
                        .unwrap();

                    let message = command.get_response(&ctx.http).await.unwrap();

                    self.responses.insert(
                        message.id,
                        ListMessageInfo {
                            user_id: command.user.id,
                            filter,
                            page: 1,
                        },
                    );
                } else {
                    log::warn!("Invalid options structure for 'list' subcommand.");
                }
            }
        }

        if let Interaction::Component(button) = interaction {
            log::info!("Processing button interaction");

            if button.data.custom_id != "next" && button.data.custom_id != "previous" {
                return;
            }

            let data = self.responses.get_mut(&button.message.id).unwrap();
            if button.user.id != data.user_id {
                // Only original author can interact with the buttons on that specific message
                button.defer(&ctx.http).await.unwrap();
                return;
            }

            if button.data.custom_id == "next" {
                data.page += 1;
            } else if button.data.custom_id == "previous" {
                data.page -= 1;
            }

            let embed = create_page_embed(data.page, PAGE_SIZE, data.filter.clone()).await;

            button
                .create_response(&ctx.http, CreateInteractionResponse::UpdateMessage(embed))
                .await
                .unwrap();
        }
    }

    async fn authorized(&self, ctx: Context, interaction: Interaction) -> bool {
        match interaction {
            Interaction::Command(command) => {
                if self.secrets.admins.contains(&command.user.id.get()) {
                    true
                } else {
                    log::warn!("Unauthorized user: {}", command.user.id);
                    log::warn!("Secret admins: {:?}", self.secrets.admins);

/*                    if !self.secrets.dev {
                        let message = MessageBuilder::new()
                            .push("You are not authorized to use this command.")
                            .build();

                        create_response(&ctx, command.clone(), message.clone()).await;
                    }*/


                    false
                }
            }
            // Button interaction handles it's self in a way that only people authorized in command can interact with the buttons
            _ => true,
        }
    }

    fn help(&self) -> Option<Vec<EmbedField>> {
        Some(
            vec![
                EmbedField::new(
                    "/contract start",
                    "Creates a new contract and immediately starts it. Takes `contract_name` and `faction_id` as arguments. \
                           \nContract name is latter use in list so I recommend naming it something meaningful like served faction name + date. Returns contract ID that can be used for ending the contract, and is to be passed to the contracted faction so they can generate report if they want to.",
                    false,
                ),
                EmbedField::new(
                    "/contract end",
                    "Ends a contract. Takes `contract_id` as argument. Contract ID is returned when creating a contract.",
                    false,
                ),
                EmbedField::new(
                    "/contract list",
                    format!("Lists all contracts. Takes `status` as argument. Status can be `active`, `ended`, or `all`. Contracts are separated in to pages by {}", PAGE_SIZE),
                    false,
                ),
            ]
        )
    }
}

async fn create_page_embed(
    page: u64,
    page_size: u64,
    filter: Option<Document>,
) -> CreateInteractionResponseMessage {
    let size = Database::get_collection_size(filter.clone()).await.unwrap();

    let options = mongodb::options::FindOptions::builder()
        .skip((page - 1) * page_size)
        .limit(page_size as i64)
        .build();

    let contracts: Vec<crate::database::structures::Contract> =
        Database::get_collection_with_filter_and_options(filter, Some(options))
            .await
            .unwrap();

    let pages = size.div_ceil(page_size);

    let mut contract_names = String::new();
    let mut started_ended = String::new();
    let mut contract_ids = String::new();

    for contract in contracts {
        contract_names.push_str(&format!("{}\n", contract.contract_name));
        started_ended.push_str(&format!(
            "{}\n",
            match contract.status {
                Status::Active => "Active",
                Status::Ended => "Ended",
            }
        ));
        contract_ids.push_str(&format!("`{}`\n", contract.contract_id));
    }

    let embed = CreateEmbed::new()
        .title("Contracts")
        .description("List of contracts")
        .fields(vec![
            ("Contract Name", contract_names, true),
            ("Status", started_ended, true),
            ("Contract ID", contract_ids, true),
        ])
        .timestamp(Utc::now())
        .footer(CreateEmbedFooter::new(format!(
            "Page {} of {}",
            page, pages
        )));

    let mut message = CreateInteractionResponseMessage::new()
        .content("List of contracts")
        .embed(embed);

    if page > 1 {
        message = message.button(
            CreateButton::new("previous")
                .style(ButtonStyle::Primary)
                .emoji(ReactionType::Unicode("⬅️".to_string())),
        );
    }

    if page < pages && pages > 1 {
        message = message.button(
            CreateButton::new("next")
                .style(ButtonStyle::Primary)
                .emoji(ReactionType::Unicode("➡️".to_string())),
        );
    }

    message
}
fn format_time(time: u64) -> String {
    format!("<t:{}:f>", time)
}

pub async fn create_response(ctx: &Context, command: CommandInteraction, message: String) {
    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(message)
                    .ephemeral(true),
            ),
        )
        .await
        .unwrap();
}

async fn generate_contract_id() -> String {
    loop {
        // Generate a 6-character alphanumeric string
        let contract_id: String = rand::thread_rng()
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

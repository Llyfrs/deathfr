use crate::bot::commands::command::Commands;
use crate::bot::tools::create_response::create_response;
use crate::bot::tools::get_player_cache::get_player_cache;
use crate::bot::Secrets;
use crate::database::structures::{Contract, ReviveEntry, Status};
use crate::database::Database;
use crate::torn_api::TornAPI;
use mongodb::bson::{doc, Bson};
use serenity::all::{
    CommandOptionType, Context, CreateCommand, CreateInteractionResponse,
    CreateInteractionResponseMessage, EmbedField, Interaction,
};
use serenity::async_trait;
use serenity::builder::{CreateCommandOption, CreateEmbed, CreateMessage};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(crate) struct Report {
    secrets: Secrets,
    api: Arc<Mutex<TornAPI>>,
}

impl Report {
    pub fn new(secrets: Secrets, api: Arc<Mutex<TornAPI>>) -> Self {
        Report { secrets, api }
    }
}

#[async_trait]
impl Commands for Report {
    fn register(&self) -> CreateCommand {
        CreateCommand::new("report")
            .description("Generate contract report")
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "contract_id",
                    "The contract ID of the player you want to report",
                )
                .required(true),
            )
    }

    async fn action(&mut self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => {
                if command.data.name.as_str() != "report" {
                    return;
                }

                let contract_id = command.data.options[0].value.as_str().unwrap();

                let contract = Database::get_collection_with_filter::<Contract>(Some(doc! {
                    "contract_id": contract_id
                }))
                .await
                .unwrap()
                .pop();

                if contract.is_none() {
                    create_response(&ctx, command, "Contract not found".to_string(), true).await;
                    return;
                }

                let contract = contract.unwrap();

                if contract.status != Status::Ended {
                    create_response(&ctx, command, "Contract is still active. Live reports will be implemented in the future hopefully.".to_string(), true).await;
                    return;
                }

                //Between start and end

                let revives = Database::get_collection_with_filter::<ReviveEntry>(Some(doc! {
                    "timestamp": {
                        "$gte": Bson::Int64(contract.started as i64),
                        "$lte": Bson::Int64(contract.ended as i64)
                    },
                    "target_faction": Bson::Int64(contract.faction_id as i64),
                    "reviver_faction": Bson::Int64(self.secrets.revive_faction as i64)
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

                let faction_data_target = self
                    .api
                    .lock()
                    .await
                    .get_faction_data(contract.faction_id)
                    .await
                    .unwrap();

                let faction_data_reviver = self
                    .api
                    .lock()
                    .await
                    .get_faction_data(self.secrets.revive_faction)
                    .await
                    .unwrap();

                let embed = CreateEmbed::new()
                    .title(contract.contract_name.clone() + " Report")
                    .description(" ")
                    .field(
                        "Reviving Faction",
                        format!(
                            "{} ({})",
                            faction_data_reviver["name"].as_str().unwrap(),
                            faction_data_reviver["ID"].as_u64().unwrap()
                        ),
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
                    .field(
                        "Final Price",
                        format!(
                            "${}",
                            format_with_commas((successful * 900000 + failed * 1000000) as u64)
                        ),
                        false,
                    );

                command
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new().embed(embed),
                        ),
                    )
                    .await
                    .expect("Failed to create response");

                // List of rewards is only for admins
                if !self.secrets.admins.contains(&command.user.id.get()) {
                    return;
                }

                let mut reward_list = Vec::new();

                for id in per_player.keys() {
                    // I could get the name, but latter when I talk it over I will
                    // probably also need the revive skill, now if the report has many players involved
                    // and it hits the rate limit, it will be a real problem as the API will freeze,
                    // here any everywhere else.
                    // TODO : I will probably need some type of cashing system for the skill the will be updated based on revive_monitor.
                    let player_data = match get_player_cache(*id, &mut *self.api.lock().await).await
                    {
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
                            format_with_commas(
                                (success * 900000 + failed_counted * 1000000) as u64
                            ),
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
                                .map(|(_, s)| s.as_str()) // Convert to &str if `s` is a `String`
                                .collect::<Vec<_>>()
                                .join("\n"),
                        );

                    command
                        .channel_id
                        .send_message(&ctx.http, CreateMessage::new().embed(embed))
                        .await
                        .unwrap();
                }
            }
            _ => return,
        }
    }

    /// Everybody should have access to this, the passed contract id will be used to generate a report
    fn is_global(&self) -> bool {
        true
    }

    fn help(&self) -> Option<Vec<EmbedField>> {
        Some(vec![EmbedField::new(
            "/report",
            "Generate contract report",
            false,
        )])
    }
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

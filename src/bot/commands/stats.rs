use crate::bot::commands::command::Commands;
use crate::bot::commands::contract::create_response;
use crate::bot::Secrets;
use crate::database::structures::ReviveEntry;
use crate::database::Database;
use crate::torn_api::TornAPI;
use mongodb::bson;
use mongodb::bson::doc;
use serenity::all::FullEvent::Message;
use serenity::all::{
    Context, CreateCommand, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage, EmbedField, Interaction, MessageBuilder,
};
use serenity::async_trait;
use std::os::linux::raw::stat;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(crate) struct Stats {
    api: Arc<Mutex<TornAPI>>,
    secrets: Secrets,
}

impl Stats {
    pub fn new(secrets: Secrets, api: Arc<Mutex<TornAPI>>) -> Self {
        Self { api, secrets }
    }
}

#[async_trait]
impl Commands for Stats {
    fn register(&self) -> CreateCommand {
        CreateCommand::new("stats").description("Get your personal reviving stats")
    }

    async fn action(&mut self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => {
                if command.data.name.as_str() != "stats" {
                    return;
                }

                log::info!("Stats command");

                let mut id = command.user.id.get();

                // I don't have any revives so I will be replaced by random player in dev mode
                if self.secrets.dev && command.user.id.get() == self.secrets.owner_id {
                    id = 2266703;
                }

                let player = self.api.lock().await.get_player_data(id).await.unwrap();

                if let Some(error) = player.get("error") {
                    log::info!("Error: {:?}", error);
                    create_response(&ctx, command, "You are not verified".to_string()).await;
                    return; // Leave the function
                }

                let filter = doc! {
                    "reviver_id": player.get("player_id").unwrap().as_i64().unwrap()
                };

                let revives: Vec<ReviveEntry> = Database::get_collection_with_filter(Some(filter))
                    .await
                    .unwrap();

                let total_revives = revives.len();

                let successful_revives = revives
                    .iter()
                    .filter(|revive| revive.result == "success")
                    .count();
                let failed_revives = revives
                    .iter()
                    .filter(|revive| revive.result == "failure")
                    .count();

                let avg_chance =
                    revives.iter().map(|revive| revive.chance).sum::<f32>() / total_revives as f32;

                let embed = CreateEmbed::default()
                    .title("Revive Stats")
                    .field("Total Revives", total_revives.to_string(), true)
                    .field("Average Chance", format!("{:.2}%", avg_chance), true)
                    .field("", "", true)
                    .field("Success", successful_revives.to_string(), true)
                    .field("Failed", failed_revives.to_string(), true)
                    .field(
                        "Success Rate",
                        format!(
                            "{:.2}%",
                            (successful_revives as f64 / total_revives as f64) * 100.0
                        ),
                        true,
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
            }
            _ => {}
        }
    }

    async fn authorized(&self, ctx: Context, interaction: Interaction) -> bool {
        match interaction {
            Interaction::Command(command) => {
                if let Some(id) = command.guild_id {
                    if id.get() == self.secrets.revive_faction_guild {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn help(&self) -> Option<Vec<EmbedField>> {
        Some(vec![EmbedField::new(
            "/stats",
            "Get your personal reviving stats over the course of your career in Lifeline",
            false,
        )])
    }
}

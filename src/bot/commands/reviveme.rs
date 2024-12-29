use crate::bot::commands::command::{interaction_command, Commands};
use crate::bot::{Bot, Secrets};
use crate::torn_api;
use crate::torn_api::TornAPI;
use serenity::all::{
    ButtonStyle, ChannelId, Content, Context, CreateButton, CreateCommand,
    CreateInteractionResponse, CreateInteractionResponseMessage, EditMessage, EmbedMessageBuilding,
    InstallationContext, InteractionContext, Message, MessageBuilder, MessageId, UserId,
};
use serenity::model::application::Interaction;
use shuttle_runtime::async_trait;
use std::collections::HashMap;
use std::mem::forget;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ReviveMe {
    api: Arc<Mutex<TornAPI>>,
    secrets: Secrets,
    responses: HashMap<UserId, Message>,
}
impl ReviveMe {
    pub fn new(secrets: Secrets, api : Arc<Mutex<TornAPI>>) -> Self {
        Self {
            api,
            secrets,
            responses: HashMap::new(),
        }
    }
}

fn player_link(id: u64) -> String {
    //https://www.torn.com/profiles.php?XID=2531272
    format!("https://www.torn.com/profiles.php?XID={}", id)
}

#[async_trait]
impl Commands for ReviveMe {
    fn register(&self) -> CreateCommand {
        CreateCommand::new("reviveme").description("Ask Lifeline for Revive")
    }
    async fn action(&mut self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => {

                // This could be an authorized function??
                if command.data.name.as_str() != "reviveme" {
                    return;
                }

                let player = self.api.lock().await.get_player_data(command.user.id.get()).await.unwrap();

                if let Some(error) = player.get("error") {
                    log::info!("Error: {:?}", error);

                    command
                        .create_response(
                            &ctx.http,
                            CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .content("You are not verified")
                                    .ephemeral(true),
                            ),
                        )
                        .await
                        .expect("Failed to create response");

                    return; // Leave the function
                }

                if player["status"]["state"].as_str().unwrap() != "Hospital" && !self.secrets.dev {
                    command
                        .create_response(
                            &ctx.http,
                            CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .content("You are not in the hospital")
                                    .ephemeral(true),
                            ),
                        )
                        .await
                        .expect("Failed to create response");

                    return; // Leave the function
                }

                command
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("Revive request sent")
                                .button(
                                    CreateButton::new("cancel_revive")
                                        .style(ButtonStyle::Danger)
                                        .label("Cancel"),
                                )
                                .ephemeral(true),
                        ),
                    )
                    .await
                    .expect("Failed to create response");

                let message = MessageBuilder::new()
                    .push("Revive request by")
                    .push_named_link(
                        format!(" {} ", player["name"].as_str().unwrap()),
                        player_link(player["player_id"].as_u64().unwrap()),
                    )
                    .role(self.secrets.revive_role)
                    .build();

                let message = ChannelId::from(self.secrets.revive_channel)
                    .say(&ctx.http, message)
                    .await
                    .unwrap();

                self.responses.insert(command.user.id, message);
            }
            Interaction::Component(mut button) => {
                if button.data.custom_id == "cancel_revive" {
                    button
                        .create_response(
                            &ctx.http,
                            CreateInteractionResponse::UpdateMessage(
                                CreateInteractionResponseMessage::new()
                                    .content("You have canceled your revive request")
                                    .components(vec![]),
                            ),
                        )
                        .await
                        .unwrap();

                    let mut message = self.responses.remove(&button.user.id).unwrap();
                    message
                        .edit(
                            &ctx.http,
                            EditMessage::new()
                                .content(message.content.clone())
                                .content("\nRevive request cancelled by user"),
                        )
                        .await
                        .unwrap();
                }
            }
            _ => {}
        }
    }

    async fn authorized(&self, ctx: Context, interaction: Interaction) -> bool {
        if !self.secrets.dev {
            match interaction {
                // IF I'm requesting revive and this is instance on server don't allow processing (This way I can test the command without sending ping to the faction server)
                Interaction::Command(command) => {
                    if command.user.id == UserId::from(self.secrets.owner_id) {
                        return false;
                    }
                }
                _ => return true,
            }
        };
        true
    }
    fn is_global(&self) -> bool {
        true
    }
}

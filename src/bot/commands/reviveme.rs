use std::collections::HashMap;
use std::mem::forget;
use crate::bot::commands::command::{interaction_command, Commands};
use crate::bot::Bot;
use crate::torn_api;
use crate::torn_api::TornAPI;
use serenity::all::{ButtonStyle, ChannelId, Content, Context, CreateButton, CreateCommand, CreateInteractionResponse, CreateInteractionResponseMessage, EditMessage, EmbedMessageBuilding, Message, MessageBuilder, MessageId, UserId};
use serenity::model::application::Interaction;
use shuttle_runtime::async_trait;

pub struct ReviveMe {
    api: TornAPI,
    revive_channel: u64,
    revive_role: u64,
    responses: HashMap<UserId, Message>
}
impl ReviveMe {
    pub fn new(revive_channel : u64, revive_role : u64) -> Self {
        let api_key = torn_api::torn_api::APIKey {
            key: "REDACTED".to_string(),
            rate_limit: 100,
            owner: "owner".to_string(),
        };

        Self {
            api: TornAPI::new(vec![api_key]),
            revive_channel,
            revive_role,
            responses: HashMap::new()
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
        CreateCommand::new("reviveme").description("Ask for a revive")
    }
    async fn action(&mut self, ctx: Context, interaction: Interaction) {

        match interaction {
            Interaction::Command(command) => {
                command.user.id; // This can be used to verify user :thumbsup:

                let player = self
                    .api
                    .get_player_data(command.user.id.get())
                    .await
                    .unwrap();

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

                //TODO: This should be != but for testing I will keep it == for now.
                if player["status"]["state"].as_str().unwrap() == "Hospital" {
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
                                        .label("Cancel")
                                )
                                .ephemeral(true),
                        ),
                    )
                    .await
                    .expect("Failed to create response");


                let message = MessageBuilder::new()
                    .push("Revive request from")
                    .push_named_link(format!(" {} ", player["name"].as_str().unwrap()),
                                     player_link(command.user.id.get())
                    )
                    .role(self.revive_role)
                    .build();

                let message = ChannelId::from(self.revive_channel).say(&ctx.http, message).await.unwrap();

                self.responses.insert(command.user.id, message);
            },
            Interaction::Component(mut button) => {
                button.create_response(&ctx.http, CreateInteractionResponse::UpdateMessage(CreateInteractionResponseMessage::new().content("You have canceled your revive request").components(
                    vec![]
                ))).await.unwrap();

                if button.data.custom_id == "cancel_revive" {
                    let mut message = self.responses.remove(&button.user.id).unwrap();
                    message.edit(
                        &ctx.http,
                        EditMessage::new().content(message.content.clone()).content("\nRevive request cancelled by user")
                    ).await.unwrap();
                }
            }
            _ => {}
        }

    }
}

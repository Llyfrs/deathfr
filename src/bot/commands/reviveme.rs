use crate::bot::commands::command::Commands;
use crate::bot::Secrets;
use crate::database::structures::Verification;
use crate::database::Database;
use crate::torn_api::TornAPI;
use mongodb::bson::doc;
use serenity::all::{
    ButtonStyle, ChannelId, CommandInteraction, Context, CreateButton, CreateCommand,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage,
    EditInteractionResponse, EditMessage, EmbedField, GuildId, InstallationContext,
    InteractionContext, Message, MessageBuilder, MessageId, RoleId, UserId,
};
use serenity::builder::CreateAllowedMentions;
use serenity::model::application::Interaction;
use shuttle_runtime::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ReviveMe {
    api: Arc<Mutex<TornAPI>>,
    secrets: Secrets,
    responses: HashMap<UserId, Message>,
    cancellation: HashMap<MessageId, CommandInteraction>,
}
impl ReviveMe {
    pub fn new(secrets: Secrets, api: Arc<Mutex<TornAPI>>) -> Self {
        Self {
            api,
            secrets,
            responses: HashMap::new(),
            cancellation: HashMap::new(),
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
        CreateCommand::new("reviveme")
            .description("Ask Lifeline for Revive")
            .add_integration_type(InstallationContext::User)
            .add_integration_type(InstallationContext::Guild)
    }
    async fn action(&mut self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => {
                log::info!("User: {:?} is requesting revive", command.user.id);

                // This could be an authorized function??
                if command.data.name.as_str() != "reviveme" {
                    return;
                }

                let filter = doc! {
                    "discord_id": command.user.id.get().to_string()
                };

                //TODO: when results is not empty, skip calling the API, this is the most to be used command and should save resources.
                let results = Database::get_collection_with_filter::<Verification>(Some(filter))
                    .await
                    .unwrap();

                let mut user_id = 0;
                let mut user_name = "".to_string();

                if results.is_empty() {
                    let player = self
                        .api
                        .lock()
                        .await
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

                    user_id = player["player_id"].as_u64().unwrap();
                    user_name = player["name"].as_str().unwrap().to_string();

                    Database::insert(Verification {
                        discord_id: command.user.id.get(),
                        torn_player_id: user_id,
                        name: user_name.clone(),
                        expire_at: chrono::Utc::now() + chrono::Duration::days(1),
                    })
                    .await
                    .unwrap();
                } else {
                    user_id = results[0].torn_player_id;
                    user_name = results[0].name.clone();
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

                let allowed_mentions = CreateAllowedMentions::new()
                    .all_roles(true)
                    .all_users(true)
                    .everyone(true);

                let message = MessageBuilder::new()
                    .push("Revive request by")
                    .push(format!(
                        " [{} [{}]]({}) ",
                        user_name,
                        user_id,
                        player_link(user_id)
                    ))
                    .role(RoleId::from(self.secrets.revive_role))
                    .build();

                let message = ctx
                    .http
                    .send_message(
                        ChannelId::from(self.secrets.revive_channel),
                        Vec::new(), // Empty Vec<CreateAttachment> if no files are being sent
                        &CreateMessage::new().content(message).button(
                            CreateButton::new("claim")
                                .style(ButtonStyle::Success)
                                .label("Claim"),
                        ),
                    )
                    .await
                    .unwrap();

                let msg = command.get_response(&ctx.http).await.unwrap();

                self.responses.insert(command.user.id, message.clone());
                self.cancellation.insert(message.id, command.clone());
            }

            Interaction::Component(button) => {
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
                                .content(format!(
                                    "{}\nRevive request cancelled by user",
                                    message.content
                                ))
                                .components(vec![]),
                        )
                        .await
                        .unwrap();
                }

                if button.data.custom_id == "claim" {
                    let command = match self.cancellation.remove(&button.message.id) {
                        Some(command) => command,
                        None => {
                            log::error!("Failed to find command for message: {:?}  (probably two people interacting at the same time)", button.message.id);

                            button
                                .create_response(
                                    &ctx.http,
                                    CreateInteractionResponse::UpdateMessage(
                                        CreateInteractionResponseMessage::new()
                                            .content("Failed to claim revive request. Somebody probably claimed it bit faster than you")
                                            .components(vec![])
                                            .ephemeral(true)
                                    ),
                                )
                                .await
                                .unwrap();

                            return;
                        }
                    };

                    command
                        .edit_response(
                            &ctx.http,
                            EditInteractionResponse::new()
                                .content("Revive request claimed")
                                .components(vec![]),
                        )
                        .await
                        .unwrap();

                    let msg = MessageBuilder::new()
                        .push(button.message.content.clone())
                        .push("\nRevive request claimed by ")
                        .user(button.user.id)
                        .build();

                    button
                        .create_response(
                            &ctx.http,
                            CreateInteractionResponse::UpdateMessage(
                                CreateInteractionResponseMessage::new()
                                    .content(msg)
                                    .components(vec![]),
                            ),
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
                    if command.user.id == UserId::from(self.secrets.owner_id)
                        && command.guild_id
                            != Option::from(GuildId::from(self.secrets.revive_faction_guild))
                    {
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
    fn help(&self) -> Option<Vec<EmbedField>> {
        Some(vec![EmbedField::new(
            "/reviveme",
            "Ask Lifeline for Revive",
            false,
        )])
    }
}

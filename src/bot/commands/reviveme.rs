use crate::bot::commands::command::Commands;
use crate::bot::Secrets;
use crate::database::structures::{Contract, Verification};
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
use crate::bot::tools::resolve_discord_verification::resolve_discord_verification;

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

fn faction_link(id: u64) -> String {
    //https://www.torn.com/factions.php?step=profile&ID=14821
    format!("https://www.torn.com/factions.php?step=profile&ID={}", id)
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


                let verification = resolve_discord_verification(
                    command.user.id.get(),
                    self.api.clone()
                ).await;


                if verification.is_none() {

                    log::warn!("Player {:?} is not verified", command.user.id);

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

                // Is not None here
                let user = verification.unwrap();


                let contract : Vec<Contract> = Database::get_collection_with_filter(
                    Some(doc! {
                            "faction_id": user.faction_id.to_string(),
                            "status": "active"
                        })).await.unwrap();

                let is_in_contract = contract.len() > 0;


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



                let mut message = MessageBuilder::new();

                message.push("Revive request by")
                    .push(format!(
                        " [{} [{}]]({}) ",
                        user.name,
                        user.torn_player_id,
                        player_link(user.torn_player_id)
                    ));

                if faction_id != 0 {
                    message.push("from")
                        .push(format!(
                            " [{}]({}) ",
                            faction_name,
                            faction_link(faction_id)
                        ));
                }

                message.role(RoleId::from(self.secrets.revive_role));

                if is_in_contract {
                    message.push_bold_line("This player is under contract ");
                    message.push(format!("Revive above {}% chance", contract[0].min_chance));
                }

                let message = message.build();

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

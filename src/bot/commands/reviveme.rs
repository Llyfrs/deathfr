use crate::bot::commands::command::Commands;
use crate::bot::Secrets;
use crate::database::structures::{Contract};
use crate::database::Database;
use crate::torn_api::TornAPI;
use mongodb::bson::doc;
use serenity::all::{ButtonStyle, ChannelId, CommandInteraction, ComponentInteraction, Context, CreateButton, CreateCommand, CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage, EditInteractionResponse, EditMessage, EmbedField, GuildId, InstallationContext, Message, MessageBuilder, MessageId, RoleId, UserId};


use serenity::model::application::Interaction;
use shuttle_runtime::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::bot::tools::create_response::create_response;
use crate::bot::tools::resolve_discord_verification::resolve_discord_verification;

pub struct ReviveMe {
    /// TornAPI instance to be used for API calls
    api: Arc<Mutex<TornAPI>>,
    secrets: Secrets,
    /// Map of messages that are sent to the reviver channel accessed by user id that asked for reviving
    responses: HashMap<UserId, Message>,
    /// Map of ephemeral messages sends it to the user when they ask for reviving
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

    /// Handles when the revive command is called
    async fn handle_revive_command(&mut self, ctx: &Context, command: CommandInteraction) {
        let verification = resolve_discord_verification(
            command.user.id.get(),
            self.api.clone()
        ).await;


        if verification.is_none() {
            log::warn!("Player {:?} is not verified", command.user.id.get());

            create_response(&ctx, command.clone(), "You are not verified".to_string(), true).await;

            return; // Leave the function
        }

        // Is not None here
        let user = verification.unwrap();


        let contract: Vec<Contract> = Database::get_collection_with_filter(
            Some(doc! {
                            "faction_id": user.faction_id as i64,
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

        /*                let allowed_mentions = CreateAllowedMentions::new()
                            .all_roles(true)
                            .all_users(true)
                            .everyone(true);

        */

        let mut message = MessageBuilder::new();

        message.push("Revive request by")
            .push(format!(
                " [{} [{}]]({}) ",
                user.name,
                user.torn_player_id,
                player_link(user.torn_player_id)
            ));

        if user.faction_id != 0 {
            message.push("from")
                .push(format!(
                    " [{}]({}) ",
                    user.faction_name,
                    faction_link(user.faction_id)
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


        self.responses.insert(command.user.id, message.clone());
        self.cancellation.insert(message.id, command.clone());
    }

    /// Handles situation where user presses the cancel button
    async fn handle_cancel_revive(&mut self, ctx: &Context, component: ComponentInteraction) {

        component
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

        let mut message = self.responses.remove(&component.user.id).unwrap();

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


    /// Handles situation where reviver presses the claim button
    async fn handle_claim_revive(&mut self, ctx: &Context, component: ComponentInteraction) {

        // We check that the message is in the cancellation map
        let maybe_command = self.cancellation.remove(&component.message.id);

        // 2. Check if it failed (was None) - This is the guard clause
        if maybe_command.is_none() {
            log::warn!("Failed to find command for message: {:?} (likely race condition on claim)", component.message.id); // Use warn or info? Error might be too strong if race conditions are expected.

            // Send an ephemeral response back to the user who clicked the button
            component.create_response(
                &ctx.http,
                // Use ::Message for a new ephemeral response to this interaction
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("Failed to claim revive request. Someone else might have claimed it already.")
                        .ephemeral(true) // Make it visible only to the clicker
                        .components(vec![]) // No components needed on the error message
                ),
            ).await.unwrap(); // Use '?' to propagate potential errors from sending the response

            // Return early. Ok(()) signifies the operation was handled (by sending the error message)
            // Or you could return a specific Err if the caller needs to know about this failure type.
            return;
        }

        // 3. If the guard didn't trigger, we know it was Some. Unwrap is safe.
        let command = maybe_command.unwrap();

        // We have a message sent to the requester and message sent to the reviver channel


        // Update sender's message so they know the request was claimed
        command
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new()
                    .content("Revive request claimed")
                    .components(vec![]),
            )
            .await
            .unwrap();

        // Update the message in the reviver channel
        let msg = MessageBuilder::new()
            .push(component.message.content.clone())
            .push("\nRevive request claimed by ")
            .user(component.user.id)
            .build();

        component
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
                if command.data.name.as_str() == "reviveme" {
                    // internally updates self.responses and self.cancellation
                    self.handle_revive_command(&ctx, command).await
                } else {
                    log::warn!("Unrecognized command: {:?}", command.data.name);
                }
            }

            Interaction::Component(button) => {
                if button.data.custom_id == "cancel_revive" {

                    log::info!("User: {:?} is canceling revive", button.user.id);

                    self.handle_cancel_revive(&ctx, button).await;

                    return;
                }

                if button.data.custom_id == "claim" {

                    log::info!("User: {:?} is claiming revive", button.user.id);

                    self.handle_claim_revive(&ctx, button).await;

                    return;
                }
            }
            _ => {}
        }
    }

    async fn authorized(&self, _ctx: Context, interaction: Interaction) -> bool {
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

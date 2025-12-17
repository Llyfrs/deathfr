use crate::bot::commands::command::Commands;
use crate::bot::Secrets;
use crate::database::structures::APIKey as DbAPIKey;
use crate::database::Database;
use crate::torn_api::torn_api::APIKey as TornApiKey;
use crate::torn_api::TornAPI;
use mongodb::bson::oid::ObjectId;
use serde_json::Value;
use serenity::all::{
    ActionRowComponent, ButtonStyle, ComponentInteraction, Context, CreateCommand,
    CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage, CreateModal,
    CreateMessage, CreateButton, EmbedField, InputTextStyle, Interaction, ModalInteraction,
};
use serenity::builder::{CreateActionRow, CreateInputText};
use serenity::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SubmitKey {
    secrets: Secrets,
    api: Arc<Mutex<TornAPI>>,
}

impl SubmitKey {
    pub fn new(secrets: Secrets, api: Arc<Mutex<TornAPI>>) -> Self {
        Self { secrets, api }
    }

    async fn handle_command(&self, ctx: &Context, command: serenity::all::CommandInteraction) {
        if command.data.name.as_str() != "submitkey" {
            return;
        }

        // First send an ephemeral explanation with a button to open the modal
        let embed = CreateEmbed::new()
            .title("Submit Torn API key")
            .description(
                "Deathfr uses Torn API keys **only** to:\n\
                 - Authenticate users when using `/reviveme`.\n\n\
                 
                Donated keys are rotated and rate limited to **10 requests per minute**. \n\n\

                Keys are **not** used to access any other infromation. Revives and other faction relevant information is collected using privatelly passed keys to me by the faction leader.\n\n\

                If you agree, click **Submit key** below to open the form.",
            );

        let message = CreateInteractionResponseMessage::new()
            .embed(embed)
            .button(
                CreateButton::new("submitkey_open_modal")
                    .label("Submit key")
                    .style(ButtonStyle::Primary),
            )
            .ephemeral(true);

        if let Err(err) = command
            .create_response(&ctx.http, CreateInteractionResponse::Message(message))
            .await
        {
            log::error!("Failed to send submitkey info message: {:?}", err);
        }
    }

    async fn handle_open_modal(&self, ctx: &Context, component: ComponentInteraction) {
        // Only handle our specific button
        if component.data.custom_id.as_str() != "submitkey_open_modal" {
            return;
        }

        let modal = CreateModal::new(
            "submitkey_modal",
            "Submit Torn API key",
        )
        .components(vec![CreateActionRow::InputText(
            CreateInputText::new(InputTextStyle::Short, "Torn API key", "api_key")
                .placeholder(
                    "Your API Key",
                )
                .required(true),
        )]);

        if let Err(err) = component
            .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
            .await
        {
            log::error!("Failed to open submitkey modal: {:?}", err);
        }
    }

    async fn handle_modal_submit(&self, ctx: &Context, modal: ModalInteraction) {
        if modal.data.custom_id.as_str() != "submitkey_modal" {
            return;
        }

        // Extract the value of the input field with custom_id "api_key"
        let mut api_key_value: Option<String> = None;

        for row in &modal.data.components {
            for comp in &row.components {
                if let ActionRowComponent::InputText(input) = comp {
                    if input.custom_id == "api_key" {
                        if let Some(value) = &input.value {
                            if !value.trim().is_empty() {
                                api_key_value = Some(value.trim().to_string());
                            }
                        }
                    }
                }
            }
        }

        let api_key = match api_key_value {
            Some(v) => v,
            None => {
                let _ = modal
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("You must provide a valid API key.")
                                .ephemeral(true),
                        ),
                    )
                    .await;
                return;
            }
        };

        let owner_name = match self.resolve_owner_name(&api_key).await {
            Some(name) => name,
            None => {
                let _ = modal
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("Invalid Torn API key. Please make sure you pasted a working key.")
                                .ephemeral(true),
                        ),
                    )
                    .await;
                return;
            }
        };

        let api_key_doc = DbAPIKey {
            id: ObjectId::new(),
            api_key: api_key.clone(),
            name: owner_name.clone(),
            valid: true,
        };

        if let Err(err) = Database::insert(api_key_doc).await {
            log::error!("Failed to insert API key into database: {:?}", err);

            let _ = modal
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content("Failed to save your API key, please try again later.")
                            .ephemeral(true),
                    ),
                )
                .await;
            return;
        }

        // Add the key to the in-memory TornAPI rotation
        {
            let mut api = self.api.lock().await;
            api.add_key(TornApiKey {
                key: api_key.clone(),
                rate_limit: 10,
                owner: owner_name.clone(),
            });
        }

        let _ = modal
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("Your API key has been saved and will be used by Lifeline.")
                        .ephemeral(true),
                ),
            )
            .await;
    }

    /// Try to resolve the owner name of a Torn API key by calling Torn API with that key.
    async fn resolve_owner_name(&self, key: &str) -> Option<String> {
        let url = format!(
            "https://api.torn.com/user/?selections=profile&key={}",
            key
        );
        let resp = reqwest::get(url).await.ok()?;
        let text = resp.text().await.ok()?;
        let json: Value = serde_json::from_str(&text).ok()?;

        if json.get("error").is_some() {
            return None;
        }

        json.get("name").and_then(|v| v.as_str()).map(|s| s.to_string())
    }
}

#[async_trait]
impl Commands for SubmitKey {
    fn register(&self) -> CreateCommand {
        CreateCommand::new("submitkey")
            .description("Submit a Torn API key to Deathfr")
    }

    async fn action(&mut self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => {
                self.handle_command(&ctx, command).await;
            }
            Interaction::Component(component) => {
                self.handle_open_modal(&ctx, component).await;
            }
            Interaction::Modal(modal) => {
                self.handle_modal_submit(&ctx, modal).await;
            }
            _ => {}
        }
    }

    async fn authorized(&self, _ctx: Context, interaction: Interaction) -> bool {
        // Only allow modal submissions that originated from the configured guild
        match interaction {
            Interaction::Command(ref command) => {
                if let Some(guild_id) = command.guild_id {
                    guild_id.get() == self.secrets.revive_faction_guild
                } else {
                    true
                }
            }
            Interaction::Component(ref component) => {
                if let Some(guild_id) = component.guild_id {
                    guild_id.get() == self.secrets.revive_faction_guild
                } else {
                    true
                }
            }
            Interaction::Modal(ref modal) => {
                if let Some(guild_id) = modal.guild_id {
                    guild_id.get() == self.secrets.revive_faction_guild
                } else {
                    true
                }
            }
            _ => true,
        }
    }

    fn help(&self) -> Option<Vec<EmbedField>> {
        Some(vec![EmbedField::new(
            "/submitkey",
            "Donate your Torn API key so Deathfr can use it for authentication of customers.",
            false,
        )])
    }
}



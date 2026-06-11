use crate::bot::data::{Context, Data, Error};
use crate::database::structures::APIKey as DbAPIKey;
use crate::database::Database;
use crate::torn_api::torn_api::APIKey as TornApiKey;
use mongodb::bson::oid::ObjectId;
use poise::CreateReply;
use serde_json::Value;
use serenity::all::{
    ActionRowComponent, ButtonStyle, ComponentInteraction, CreateButton, CreateEmbed,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateModal, InputTextStyle,
    ModalInteraction,
};
use serenity::builder::{CreateActionRow, CreateInputText};

/// Submit a Torn API key to Deathfr
#[poise::command(slash_command)]
pub async fn submitkey(ctx: Context<'_>) -> Result<(), Error> {
    // Only allow usage from the configured guild (DMs are fine)
    if let Some(guild_id) = ctx.guild_id() {
        if guild_id.get() != ctx.data().secrets.revive_faction_guild {
            return Ok(());
        }
    }

    // First send an ephemeral explanation with a button to open the modal
    let embed = CreateEmbed::new()
        .title("Submit Torn API key")
        .description(
            "Deathfr uses Torn API keys **only** to:\n\
             - Authenticate users when using `/reviveme`.\n\
             - Some simple / basic requests to check the API key validity. \n\n\
             
            Donated keys are rotated and rate limited to **10 requests per minute**. \n\n\

            Keys are **not** used to access any other infromation. Revives and other faction relevant information is collected using privatelly passed keys to me by the faction leader.\n\n\

            If you agree, click **Submit key** below to open the form.",
        );

    let reply = CreateReply::default()
        .embed(embed)
        .components(vec![CreateActionRow::Buttons(vec![CreateButton::new(
            "submitkey_open_modal",
        )
        .label("Submit key")
        .style(ButtonStyle::Primary)])])
        .ephemeral(true);

    if let Err(err) = ctx.send(reply).await {
        log::error!("Failed to send submitkey info message: {:?}", err);
    }

    Ok(())
}

/// Returns true when the interaction may proceed: it either comes from the configured guild or a DM.
fn allowed_guild(data: &Data, guild_id: Option<serenity::all::GuildId>) -> bool {
    match guild_id {
        Some(guild_id) => guild_id.get() == data.secrets.revive_faction_guild,
        None => true,
    }
}

pub async fn handle_open_modal(
    ctx: &serenity::all::Context,
    data: &Data,
    component: &ComponentInteraction,
) -> Result<(), Error> {
    if !allowed_guild(data, component.guild_id) {
        return Ok(());
    }

    let modal = CreateModal::new("submitkey_modal", "Submit Torn API key").components(vec![
        CreateActionRow::InputText(
            CreateInputText::new(InputTextStyle::Short, "Torn API key", "api_key")
                .placeholder("Your API Key")
                .required(true),
        ),
    ]);

    if let Err(err) = component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await
    {
        log::error!("Failed to open submitkey modal: {:?}", err);
    }

    Ok(())
}

pub async fn handle_modal_submit(
    ctx: &serenity::all::Context,
    data: &Data,
    modal: &ModalInteraction,
) -> Result<(), Error> {
    if !allowed_guild(data, modal.guild_id) {
        return Ok(());
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

    let Some(api_key) = api_key_value else {
        respond_ephemeral(ctx, modal, "You must provide a valid API key.").await;
        return Ok(());
    };

    let Some(owner_name) = resolve_owner_name(&api_key).await else {
        respond_ephemeral(
            ctx,
            modal,
            "Invalid Torn API key. Please make sure you pasted a working key.",
        )
        .await;
        return Ok(());
    };

    let api_key_doc = DbAPIKey {
        id: ObjectId::new(),
        api_key: api_key.clone(),
        name: owner_name.clone(),
        valid: true,
    };

    if let Err(err) = Database::insert(api_key_doc).await {
        log::error!("Failed to insert API key into database: {:?}", err);
        respond_ephemeral(
            ctx,
            modal,
            "Failed to save your API key, please try again later.",
        )
        .await;
        return Ok(());
    }

    // Add the key to the in-memory TornAPI rotation
    {
        let mut api = data.torn_api.lock().await;
        api.add_key(TornApiKey {
            key: api_key.clone(),
            rate_limit: 10,
            owner: owner_name.clone(),
        });
    }

    respond_ephemeral(
        ctx,
        modal,
        "Your API key has been saved and will be used by Lifeline.",
    )
    .await;

    Ok(())
}

async fn respond_ephemeral(ctx: &serenity::all::Context, modal: &ModalInteraction, content: &str) {
    let _ = modal
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .ephemeral(true),
            ),
        )
        .await;
}

/// Try to resolve the owner name of a Torn API key by calling Torn API with that key.
async fn resolve_owner_name(key: &str) -> Option<String> {
    let url = format!("https://api.torn.com/user/?selections=profile&key={}", key);
    let resp = reqwest::get(url).await.ok()?;
    let text = resp.text().await.ok()?;
    let json: Value = serde_json::from_str(&text).ok()?;

    if json.get("error").is_some() {
        return None;
    }

    json.get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

use crate::bot::data::{Context, Data, Error};
use crate::bot::tools::resolve_discord_verification::resolve_discord_verification;
use crate::database::structures::Contract;
use crate::database::Database;
use mongodb::bson::doc;
use poise::CreateReply;
use serenity::all::{
    ButtonStyle, ChannelId, ComponentInteraction, CreateActionRow, CreateButton,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage,
    EditInteractionResponse, EditMessage, MessageBuilder, RoleId,
};

/// Ask Lifeline for Revive
#[poise::command(slash_command, install_context = "Guild|User")]
pub async fn reviveme(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let secrets = &data.secrets;

    log::info!("User: {:?} is requesting revive", ctx.author().id);

    // In prod the owner is not allowed to trigger pings from outside the faction guild,
    // this way the command can be tested without pinging the faction server.
    if !secrets.dev
        && ctx.author().id.get() == secrets.owner_id
        && ctx.guild_id().map(|g| g.get()) != Some(secrets.revive_faction_guild)
    {
        return Ok(());
    }

    let verification =
        resolve_discord_verification(ctx.author().id.get(), data.torn_api.clone()).await;

    let Some(user) = verification else {
        log::warn!("Player {:?} is not verified", ctx.author().id.get());
        ctx.send(
            CreateReply::default()
                .content("You are not verified")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    };

    let contract: Vec<Contract> = Database::get_collection_with_filter(Some(doc! {
        "faction_id": user.faction_id as i64,
        "status": "active"
    }))
    .await
    .unwrap();

    let is_in_contract = contract.len() > 0;

    ctx.send(
        CreateReply::default()
            .content("Revive request sent")
            .components(vec![CreateActionRow::Buttons(vec![CreateButton::new(
                "cancel_revive",
            )
            .style(ButtonStyle::Danger)
            .label("Cancel")])])
            .ephemeral(true),
    )
    .await?;

    let mut message = MessageBuilder::new();

    message.push("Revive request by").push(format!(
        " [{} [{}]]({}) ",
        user.name,
        user.torn_player_id,
        player_link(user.torn_player_id)
    ));

    if user.faction_id != 0 {
        message.push("from").push(format!(
            " [{}]({}) ",
            user.faction_name,
            faction_link(user.faction_id)
        ));
    }

    message.role(RoleId::from(secrets.revive_role));

    if is_in_contract {
        message.push_bold("\nThis player is under contract ");
        message.push(format!("Revive above {}% chance", contract[0].min_chance));
    }

    let message = message.build();

    let message = ctx
        .serenity_context()
        .http
        .send_message(
            ChannelId::from(secrets.revive_channel),
            Vec::new(), // Empty Vec<CreateAttachment> if no files are being sent
            &CreateMessage::new().content(message).button(
                CreateButton::new("claim")
                    .style(ButtonStyle::Success)
                    .label("Claim"),
            ),
        )
        .await?;

    // Keep the original interaction around so the ephemeral response can be edited
    // when the request is claimed from the reviver channel.
    let poise::Context::Application(app_ctx) = ctx else {
        return Ok(());
    };
    let command = app_ctx.interaction.clone();

    data.revive_responses
        .lock()
        .await
        .insert(command.user.id, message.clone());
    data.revive_cancellations
        .lock()
        .await
        .insert(message.id, command);

    Ok(())
}

/// Handles situation where user presses the cancel button
pub async fn handle_cancel_revive(
    ctx: &serenity::all::Context,
    data: &Data,
    component: &ComponentInteraction,
) -> Result<(), Error> {
    log::info!("User: {:?} is canceling revive", component.user.id);

    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content("You have canceled your revive request")
                    .components(vec![]),
            ),
        )
        .await?;

    let removed = data
        .revive_responses
        .lock()
        .await
        .remove(&component.user.id);

    let Some(mut message) = removed else {
        log::warn!(
            "No tracked revive request found for user {:?}",
            component.user.id
        );
        return Ok(());
    };

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
        .await?;

    Ok(())
}

/// Handles situation where reviver presses the claim button
pub async fn handle_claim_revive(
    ctx: &serenity::all::Context,
    data: &Data,
    component: &ComponentInteraction,
) -> Result<(), Error> {
    log::info!("User: {:?} is claiming revive", component.user.id);

    // We check that the message is in the cancellation map
    let maybe_command = data
        .revive_cancellations
        .lock()
        .await
        .remove(&component.message.id);

    let Some(command) = maybe_command else {
        log::warn!(
            "Failed to find command for message: {:?} (likely race condition on claim)",
            component.message.id
        );

        // Send an ephemeral response back to the user who clicked the button
        component
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("Failed to claim revive request. Someone else might have claimed it already.")
                        .ephemeral(true)
                        .components(vec![]),
                ),
            )
            .await?;

        return Ok(());
    };

    // Update sender's message so they know the request was claimed
    command
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .content("Revive request claimed")
                .components(vec![]),
        )
        .await?;

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
        .await?;

    Ok(())
}

fn player_link(id: u64) -> String {
    //https://www.torn.com/profiles.php?XID=2531272
    format!("https://www.torn.com/profiles.php?XID={}", id)
}

fn faction_link(id: u64) -> String {
    //https://www.torn.com/factions.php?step=profile&ID=14821
    format!("https://www.torn.com/factions.php?step=profile&ID={}", id)
}

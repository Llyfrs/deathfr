use crate::bot::auth::{level_of, AccessLevel};
use crate::bot::data::{Context, Error};
use poise::CreateReply;
use serenity::all::GuildChannel;

/// Configure the live channel showing all active and upcoming contracts
#[poise::command(
    slash_command,
    guild_only,
    rename = "live-channel",
    subcommands("set", "disable", "refresh")
)]
pub async fn live_channel(_ctx: Context<'_>) -> Result<(), Error> {
    // Parent command of subcommands, never invoked directly.
    Ok(())
}

/// Returns false (and replies) when the invoking user is not an admin.
async fn ensure_admin(ctx: &Context<'_>) -> Result<bool, Error> {
    if level_of(ctx) >= AccessLevel::Admin {
        return Ok(true);
    }

    ctx.send(
        CreateReply::default()
            .content("You are not authorized to use this command.")
            .ephemeral(true),
    )
    .await?;

    Ok(false)
}

async fn reply(ctx: &Context<'_>, content: impl Into<String>) -> Result<(), Error> {
    ctx.send(CreateReply::default().content(content).ephemeral(true))
        .await?;
    Ok(())
}

/// Set the channel the bot keeps filled with active and upcoming contracts
#[poise::command(slash_command, guild_only)]
pub async fn set(
    ctx: Context<'_>,
    #[description = "Dedicated channel; anything else posted there gets deleted"]
    #[channel_types("Text", "News")]
    channel: GuildChannel,
) -> Result<(), Error> {
    if !ensure_admin(&ctx).await? {
        return Ok(());
    }

    let Some(guild_id) = ctx.guild_id() else {
        return Ok(());
    };

    if channel.guild_id != guild_id {
        reply(&ctx, "The live channel has to be in this server.").await?;
        return Ok(());
    }

    ctx.defer_ephemeral().await?;

    let http = ctx.serenity_context().http.clone();
    match ctx
        .data()
        .live_board
        .set_channel(&http, guild_id, channel.id)
        .await
    {
        Ok(report) => {
            reply(
                &ctx,
                format!(
                    "<#{}> is now the live contract channel.\n\
                     Messages posted from now on that aren't part of the board are deleted; \
                     older messages were left untouched. A dedicated, read-only channel works best.\n\n{}",
                    channel.id,
                    report.summary()
                ),
            )
            .await?;
        }
        Err(e) => {
            log::error!("Failed to set live channel for guild {guild_id}: {e:#}");
            reply(
                &ctx,
                format!(
                    "<#{}> was saved as the live channel, but filling it failed: {e:#}\n\
                     The bot keeps retrying in the background.",
                    channel.id
                ),
            )
            .await?;
        }
    }

    Ok(())
}

/// Stop maintaining the live channel and remove the board messages
#[poise::command(slash_command, guild_only)]
pub async fn disable(ctx: Context<'_>) -> Result<(), Error> {
    if !ensure_admin(&ctx).await? {
        return Ok(());
    }

    let Some(guild_id) = ctx.guild_id() else {
        return Ok(());
    };

    ctx.defer_ephemeral().await?;

    let http = ctx.serenity_context().http.clone();
    let content = match ctx.data().live_board.disable(&http, guild_id).await {
        Ok(true) => "Live channel disabled.".to_string(),
        Ok(false) => "No live channel is configured in this server.".to_string(),
        Err(e) => {
            log::error!("Failed to disable live channel for guild {guild_id}: {e:#}");
            format!("Failed to disable the live channel: {e:#}")
        }
    };

    reply(&ctx, content).await
}

/// Refresh the live channel now
#[poise::command(slash_command, guild_only)]
pub async fn refresh(ctx: Context<'_>) -> Result<(), Error> {
    if !ensure_admin(&ctx).await? {
        return Ok(());
    }

    let Some(guild_id) = ctx.guild_id() else {
        return Ok(());
    };

    ctx.defer_ephemeral().await?;

    let http = ctx.serenity_context().http.clone();
    let content = match ctx.data().live_board.refresh_guild(&http, guild_id).await {
        Ok(Some(report)) => format!("Live channel refreshed. {}", report.summary()),
        Ok(None) => "No live channel is configured in this server. Use `/live-channel set` first."
            .to_string(),
        Err(e) => {
            log::error!("Failed to refresh live channel for guild {guild_id}: {e:#}");
            format!("Failed to refresh the live channel: {e:#}")
        }
    };

    reply(&ctx, content).await
}

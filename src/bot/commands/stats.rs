use crate::bot::auth::{level_of, AccessLevel};
use crate::bot::data::{Context, Error};
use crate::bot::tools::resolve_discord_verification::resolve_discord_verification;
use crate::database::structures::ReviveEntry;
use crate::database::Database;
use mongodb::bson::doc;
use poise::CreateReply;
use serenity::all::CreateEmbed;

/// Get your personal reviving stats
#[poise::command(slash_command)]
pub async fn stats(ctx: Context<'_>) -> Result<(), Error> {
    if level_of(&ctx) < AccessLevel::FactionGuild {
        ctx.send(
            CreateReply::default()
                .content("This command can only be used in the faction server.")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    let secrets = &ctx.data().secrets;
    let id = ctx.author().id.get();

    let verification = resolve_discord_verification(id, ctx.data().torn_api.clone()).await;

    let Some(mut player) = verification else {
        log::info!("User {} is not verified", id);
        ctx.send(
            CreateReply::default()
                .content("You are not verified")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    };

    // I don't have any revives so I will be replaced by random player in dev mode
    if secrets.dev && id == secrets.owner_id {
        player.torn_player_id = 2266703;
    }

    let filter = doc! {
        "reviver_id": player.torn_player_id as i64
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

    ctx.send(CreateReply::default().embed(embed)).await?;

    Ok(())
}

use crate::bot::data::{Context, Error};
use poise::CreateReply;

/// Learn how to request a revive contract from Cerberus Alliance
#[poise::command(slash_command, install_context = "Guild|User")]
pub async fn needcontract(ctx: Context<'_>) -> Result<(), Error> {
    ctx.send(
        CreateReply::default().content(
            "**How to request a contract**\n\
            1. Join the Cerberus Alliance Discord: https://discord.gg/SXaaYyGGeA\n\
            2. Go to the #rev-contract-request channel\n\
            3. Ping `@Contract` to request a contract",
        ),
    )
    .await?;

    Ok(())
}

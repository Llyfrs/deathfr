use crate::bot::auth::{level_of, AccessLevel};
use crate::bot::commands::contract::PAGE_SIZE;
use crate::bot::data::{Context, Error};
use poise::CreateReply;
use serenity::all::{CreateEmbed, CreateEmbedFooter};

/// Get a list of all available commands
#[poise::command(slash_command)]
pub async fn help(ctx: Context<'_>) -> Result<(), Error> {
    let level = level_of(&ctx);

    let mut fields: Vec<(String, String, bool)> = Vec::new();

    fields.push((
        "/reviveme".to_string(),
        "Ask Lifeline for Revive".to_string(),
        false,
    ));

    if level >= AccessLevel::Admin {
        fields.push((
            "/start-contract-interactive".to_string(),
            "**Experimental.** Step-by-step contract creation in a single ephemeral message using buttons and forms. \
             Guides you through name, faction, min chance, pricing, optional faction cut and start time, with back/cancel/skip. \
             Existing `/contract start` is unchanged.".to_string(),
            false,
        ));
        fields.push((
            "/contract start".to_string(),
            "Creates a new contract and starts it immediately unless `start_time` is provided. Takes `contract_name`, `faction_id`, and `min_chance` as arguments. \n \
                 * `contract_name` is used as a identifier in list so I recommend naming it something meaningful like served faction name + date. \n \
                 * `faction_id` is the faction you want to track revives for (if **both defense and offensive revives** are provided two different contracts need to be made \n\
                 * `min_chance` is the minimum revive chance of success to count for payment \n\
                 * `pricing_type` selects the rate tier: `external` ($1M/$750k) or `inter_alliance` ($800k/$550k) \n\
                 * `faction_cut` is the cut the faction gets from the contract (defaults to 10% for external, 0% for inter_alliance) \n\
                * `start_time` is optional and must use `YYYY-MM-DD HH:MM` in UTC. Future times create a pending contract. \n\
                 Returns contract ID that can be used for ending the contract, and is to be passed to the contracted faction so they can generate report if they want to."
                .to_string(),
            false,
        ));
        fields.push((
            "/contract end".to_string(),
            "Ends a contract. Takes `contract_id` as argument. Contract ID is returned when creating a new contract."
                .to_string(),
            false,
        ));
        fields.push((
            "/contract list".to_string(),
            format!("Lists all contracts. Takes `status` as argument. Status can be `active`, `pending`, `ended`, or `all`. Contracts are separated in to pages by {}", PAGE_SIZE),
            false,
        ));
    }

    if level >= AccessLevel::FactionGuild {
        fields.push((
            "/stats".to_string(),
            "Get your personal reviving stats over the course of your career in Lifeline"
                .to_string(),
            false,
        ));
    }

    fields.push((
        "/report".to_string(),
        "Generate contract report".to_string(),
        false,
    ));

    if level >= AccessLevel::FactionGuild {
        fields.push((
            "/submitkey".to_string(),
            "Donate your Torn API key so Deathfr can use it for authentication of customers."
                .to_string(),
            false,
        ));
    }

    fields.push((
        "/help".to_string(),
        "Get a list of all available commands".to_string(),
        false,
    ));

    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::default()
                .title("Help")
                .description("List of all commands **available to you**.")
                .fields(fields)
                .footer(CreateEmbedFooter::new("Bot author: Llyfr [2531272]")),
        ),
    )
    .await?;

    Ok(())
}

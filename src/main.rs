mod bot;
mod database;
mod torn_api;

use crate::bot::{Bot, Secrets};
use crate::database::structures::{APIKey, Player};
use crate::database::Database;
use anyhow::Context as _;
use log;
use mongodb::bson::doc;
use serenity::prelude::*;
use shuttle_runtime::SecretStore;

use crate::torn_api::TornAPI;
#[shuttle_runtime::main]
async fn serenity(
    #[shuttle_runtime::Secrets] secrets: SecretStore,
) -> shuttle_serenity::ShuttleSerenity {
    Database::init(
        secrets
            .get("DATABASE_URL")
            .context("'MONGODB_URI' was not found")?,
    )
    .await
    .expect("Error initializing database");

    /*    let keys : Vec<APIKey> = Database::get_collection().await.unwrap();

    for key in keys {
        log::info!("API Key: {:?}",
            key.api_key
        );
    }*/

    let players: Vec<Player> = Database::get_collection_with_filter(Some(doc! {
        "uid": 61399
    }))
    .await
    .unwrap();

    println!("Players: {:?}", players);

    for player in players {
        log::info!("Player: {:?}", player.name);
    }

    let api_key = torn_api::torn_api::APIKey {
        key: "REDACTED".to_string(),
        rate_limit: 100,
        owner: "owner".to_string(),
    };

    let mut api = TornAPI::new(vec![api_key]);

    /*    let revives = api.get_revives(1735112864).await;

    for revive in revives {
        log::info!("Revive: {:?}", revive);
    }*/

    let secret = Secrets {
        revive_channel: secrets
            .get("REVIVE_CHANNEL")
            .context("'REVIVE_CHANNEL' was not found")?
            .parse()
            .unwrap(),
        revive_role: secrets
            .get("REVIVE_ROLE")
            .context("'REVIVE_ROLE' was not found")?
            .parse()
            .unwrap(),
        revive_faction_guild: secrets
            .get("REVIVE_FACTION_GUILD_ID")
            .context("'REVIVE_FACTION_GUILD_ID' was not found")?
            .parse()
            .unwrap(),
        revive_faction: secrets
            .get("REVIVE_FACTION")
            .context("'REVIVE_FACTION' was not found")?
            .parse()
            .unwrap(),
        owner_id: secrets
            .get("OWNER_ID")
            .context("'OWNER_ID' was not found")?
            .parse()
            .unwrap(),
    };

    let mut bot = Bot::new(secret).await;
    bot.torn_api = api;


    // Get the discord token set in `Secrets.toml`
    let token = secrets
        .get("DISCORD_TOKEN")
        .context("'DISCORD_TOKEN' was not found")?;

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let client = Client::builder(&token, intents)
        .event_handler(bot)
        .await
        .expect("Err creating client");

    Ok(client.into())
}

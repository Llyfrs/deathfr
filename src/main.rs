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

use crate::torn_api::{revive_monitor, TornAPI};

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

    let api_keys = Database::get_collection::<APIKey>().await.unwrap();

    let mut api_keys: Vec<torn_api::torn_api::APIKey> = api_keys
        .into_iter()
        .filter_map(|key| {
            if key.valid {
                Some(torn_api::torn_api::APIKey {
                    key: key.api_key,
                    rate_limit: 1,
                    owner: key.name,
                })
            } else {
                None
            }
        })
        .collect();

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
        revive_faction_api_key: secrets
            .get("REVIVE_FACTION_API_KEY")
            .context("'REVIVE_FACTION_API_KEY' was not found")?,
        test_api_key: secrets
            .get("TEST_API_KEY")
            .context("'TEST_API_KEY' was not found")?,
        dev: secrets
            .get("DEV")
            .context("'DEV' was not found")?
            .parse()
            .unwrap(),
        admins: secrets
            .get("ADMINS")
            .context("'ADMINS' was not found")?
            .split(',')
            .map(|x| x.trim().parse().unwrap())
            .collect(),
    };

    // let's waste only my API call for testing
    if secret.dev {
        api_keys = vec![torn_api::torn_api::APIKey {
            key: secret.test_api_key.clone(),
            rate_limit: 100,
            owner: "Test Key (Llyfr)".to_string(),
        }];
    }

    let mut api = TornAPI::new(api_keys);

    api.set_name("Deathfr".to_string());

    if secret.dev {
        log::info!("Running in dev mode");
    }

    tokio::spawn(revive_monitor(secret.revive_faction_api_key.clone()));

    let mut bot = Bot::new(secret, api).await;

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

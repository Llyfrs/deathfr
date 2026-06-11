mod bot;
mod database;
mod torn_api;

use crate::bot::commands;
use crate::bot::handler::event_handler;
use crate::bot::{Data, Secrets};
use crate::database::structures::APIKey;
use crate::database::Database;
use anyhow::Context as _;
use log;
use serde::Deserialize;
use serenity::all::GuildId;
use serenity::prelude::*;
use std::{env, fs};

use crate::torn_api::{revive_monitor, TornAPI};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct SecretsConfig {
    discord_token: String,
    database_url: String,

    revive_channel: String,
    revive_role: String,
    revive_faction_guild_id: String,
    owner_id: String,
    admins: Vec<String>,

    revive_faction: String,
    revive_faction_api_key: String,

    test_api_key: String,

    // Keep this as string so existing Secrets.*.toml values like `DEV = "true"` still work.
    dev: String,
}

fn select_secrets_path() -> anyhow::Result<String> {
    let args: Vec<String> = env::args().skip(1).collect();

    // `--secrets <path>` overrides everything
    if let Some(i) = args.iter().position(|a| a == "--secrets") {
        let path = args
            .get(i + 1)
            .context("Expected a path after '--secrets'")?;
        return Ok(path.clone());
    }

    // Explicit dev/prod flags
    if args.iter().any(|a| a == "--dev") {
        return Ok("Secrets.dev.toml".to_string());
    }
    if args.iter().any(|a| a == "--prod") {
        return Ok("Secrets.toml".to_string());
    }

    // Fallbacks: prefer prod if present, otherwise dev
    if std::path::Path::new("Secrets.toml").exists() {
        Ok("Secrets.toml".to_string())
    } else if std::path::Path::new("Secrets.dev.toml").exists() {
        Ok("Secrets.dev.toml".to_string())
    } else {
        anyhow::bail!(
            "No secrets file found. Create 'Secrets.toml' or 'Secrets.dev.toml' (or pass '--secrets <path>')."
        );
    }
}

fn parse_u64(field: &'static str, s: &str) -> anyhow::Result<u64> {
    s.trim()
        .parse::<u64>()
        .with_context(|| format!("Invalid u64 for {field}: '{s}'"))
}

fn parse_bool(field: &'static str, s: &str) -> anyhow::Result<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" => Ok(true),
        "false" | "0" | "no" | "n" => Ok(false),
        other => anyhow::bail!("Invalid bool for {field}: '{other}'"),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,deathfr=info")).init();

    let secrets_path = select_secrets_path()?;
    let secrets_raw = fs::read_to_string(&secrets_path)
        .with_context(|| format!("Failed to read secrets file '{secrets_path}'"))?;
    let cfg: SecretsConfig = toml::from_str(&secrets_raw)
        .with_context(|| format!("Failed to parse TOML secrets file '{secrets_path}'"))?;

    Database::init(
        cfg.database_url.clone(),
    )
    .await
    .expect("Error initializing database");

    Database::ensure_indexes()
        .await
        .expect("Error ensuring database indexes");

    let api_keys = Database::get_collection::<APIKey>().await.unwrap();

    let mut api_keys: Vec<torn_api::torn_api::APIKey> = api_keys
        .into_iter()
        .filter_map(|key| {
            if key.valid {
                Some(torn_api::torn_api::APIKey {
                    key: key.api_key,
                    rate_limit: 10,
                    owner: key.name,
                })
            } else {
                None
            }
        })
        .collect();

    let secret = Secrets {
        revive_channel: parse_u64("REVIVE_CHANNEL", &cfg.revive_channel)?,
        revive_role: parse_u64("REVIVE_ROLE", &cfg.revive_role)?,
        revive_faction_guild: parse_u64("REVIVE_FACTION_GUILD_ID", &cfg.revive_faction_guild_id)?,
        revive_faction: parse_u64("REVIVE_FACTION", &cfg.revive_faction)?,
        owner_id: parse_u64("OWNER_ID", &cfg.owner_id)?,
        revive_faction_api_key: cfg.revive_faction_api_key.clone(),
        test_api_key: cfg.test_api_key.clone(),
        dev: parse_bool("DEV", &cfg.dev)?,
        admins: cfg
            .admins
            .iter()
            .map(|x| parse_u64("ADMINS[]", x))
            .collect::<anyhow::Result<Vec<u64>>>()?,
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

    let data = Data::new(secret.clone(), api);

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::reviveme::reviveme(),
                commands::contract::contract(),
                commands::stats::stats(),
                commands::report::report(),
                commands::submitkey::submitkey(),
                commands::help::help(),
            ],
            event_handler: |ctx, event, framework, data| {
                Box::pin(event_handler(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(move |ctx, ready, _framework| {
            Box::pin(async move {
                log::info!("{} is connected!", ready.user.name);

                let secrets = &data.secrets;

                // Guilds the guild-only commands get registered in. Add more ids here if needed.
                let guild_ids: Vec<GuildId> = vec![GuildId::from(secrets.revive_faction_guild)];

                // Clears all commands when deployed for cleanup, should not be used in dev mode?
                if !secrets.dev {
                    for guild_id in &guild_ids {
                        let cmds = ctx.http.get_guild_commands(*guild_id).await?;
                        for cmd in cmds {
                            ctx.http.delete_guild_command(*guild_id, cmd.id).await?;
                        }
                    }
                    log::info!("All old commands cleared!");
                }

                let global_commands = poise::builtins::create_application_commands(&[
                    commands::reviveme::reviveme(),
                    commands::report::report(),
                    commands::help::help(),
                ]);

                let guild_commands = poise::builtins::create_application_commands(&[
                    commands::contract::contract(),
                    commands::stats::stats(),
                    commands::submitkey::submitkey(),
                ]);

                serenity::all::Command::set_global_commands(&ctx.http, global_commands).await?;

                for guild_id in &guild_ids {
                    guild_id.set_commands(&ctx.http, guild_commands.clone()).await?;
                }

                log::info!("All commands registered!");

                {
                    let revive_faction = secrets.revive_faction;
                    let revive_faction_api_key = secrets.revive_faction_api_key.clone();

                    tokio::spawn(async move {
                        revive_monitor(revive_faction_api_key, revive_faction).await;
                    });
                }

                log::info!("The bot is ready to go!");

                Ok(data)
            })
        })
        .build();

    // Get the discord token set in `Secrets.toml`
    let token = cfg.discord_token.clone();

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILDS;

    let mut client = Client::builder(&token, intents)
        .framework(framework)
        .await
        .expect("Err creating client");

    client.start().await?;
    Ok(())
}

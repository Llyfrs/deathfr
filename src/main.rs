mod bot;
mod database;
mod pricing;
mod torn_api;

use crate::bot::commands;
use crate::bot::handler::event_handler;
use crate::bot::{Data, Secrets};
use crate::database::structures::APIKey;
use crate::database::Database;
use log;
use serenity::all::GuildId;
use serenity::prelude::*;
use std::env;

use crate::torn_api::{ReviveMonitor, ReviveSourceConfig, TornAPI};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,deathfr=info")).init();

    let loaded = Secrets::load()?;

    Database::init(loaded.database_url.clone())
        .await
        .expect("Error initializing database");

    Database::ensure_indexes()
        .await
        .expect("Error ensuring database indexes");

    let api_keys = Database::get_collection::<APIKey>().await.unwrap();

    let mut api_keys: Vec<torn_api::APIKey> = api_keys
        .into_iter()
        .filter_map(|key| {
            if key.valid {
                Some(torn_api::APIKey {
                    key: key.api_key,
                    rate_limit: 10,
                    owner: key.name,
                })
            } else {
                None
            }
        })
        .collect();

    let secret = loaded.secrets;

    // let's waste only my API call for testing
    if secret.dev {
        api_keys = vec![torn_api::APIKey {
            key: secret.test_api_key.clone(),
            rate_limit: 100,
            owner: "Test Key (Llyfr)".to_string(),
        }];
    }

    let api = TornAPI::new(api_keys);

    if secret.dev {
        log::info!("Running in dev mode");
    }

    let revive_sources = if secret.revive_sources.is_empty() {
        vec![ReviveSourceConfig {
            api_key: secret.revive_faction_api_key.clone(),
            faction_ids: vec![secret.revive_faction],
        }]
    } else {
        secret.revive_sources.clone()
    };

    // Wakes the live board whenever revives were synced or contracts changed.
    let live_board_notify = Arc::new(tokio::sync::Notify::new());
    let revive_monitor = Arc::new(ReviveMonitor::new(revive_sources, live_board_notify.clone()));
    let data = Data::new(secret.clone(), api, revive_monitor.clone(), live_board_notify);

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::reviveme::reviveme(),
                commands::contract::contract(),
                commands::contract_wizard::start_contract_interactive(),
                commands::stats::stats(),
                commands::report::report(),
                commands::submitkey::submitkey(),
                commands::help::help(),
                commands::new_contract::new_contract(),
                commands::live_channel::live_channel(),
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
                let guild_ids: Vec<GuildId> = secrets
                    .revive_faction_guilds
                    .iter()
                    .copied()
                    .map(GuildId::from)
                    .collect();

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
                    commands::new_contract::new_contract(),
                ]);

                let guild_commands = poise::builtins::create_application_commands(&[
                    commands::contract::contract(),
                    commands::contract_wizard::start_contract_interactive(),
                    commands::stats::stats(),
                    commands::submitkey::submitkey(),
                    commands::live_channel::live_channel(),
                ]);

                serenity::all::Command::set_global_commands(&ctx.http, global_commands).await?;

                for guild_id in &guild_ids {
                    guild_id.set_commands(&ctx.http, guild_commands.clone()).await?;
                }

                log::info!("All commands registered!");

                tokio::spawn({
                    let monitor = data.revive_monitor.clone();
                    async move {
                        monitor.run_loop().await;
                    }
                });

                tokio::spawn({
                    let live_board = data.live_board.clone();
                    let http = ctx.http.clone();
                    async move {
                        live_board.run_loop(http).await;
                    }
                });

                log::info!("The bot is ready to go!");

                if let Err(e) = bot::startup::notify_startup(&ctx, secrets).await {
                    log::error!("Failed to send startup notification: {e:#}");
                }

                Ok(data)
            })
        })
        .build();

    let token = loaded.discord_token;

    // Set gateway intents, which decides what events the bot will be notified about
    // GUILD_MESSAGES (not privileged) lets the live channel delete foreign messages right away.
    let intents = GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGES;

    let mut client = Client::builder(&token, intents)
        .framework(framework)
        .await
        .expect("Err creating client");

    client.start().await?;
    Database::close().await;
    Ok(())
}

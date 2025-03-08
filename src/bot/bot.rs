use log::info;
use serenity::all::{Context, EventHandler, GuildId, Message, Ready};
use serenity::model::application::{Interaction};
use shuttle_runtime::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use tracing::error;

use crate::bot::commands::command::Commands;
use crate::bot::commands::contract::Contract;
use crate::bot::commands::help::Help;
use crate::bot::commands::report::Report;
use crate::bot::commands::stats::Stats;
// Import the trait
use crate::bot::commands::reviveme::ReviveMe;
use crate::torn_api::TornAPI;

//**
//  Holds all the required secrets for the bot to work
// *
#[derive(Debug, Clone)]
pub struct Secrets {
    pub revive_channel: u64,
    pub revive_role: u64,
    pub revive_faction_guild: u64,
    pub revive_faction: u64,
    pub owner_id: u64,
    pub admins: Vec<u64>,
    pub revive_faction_api_key: String,
    pub test_api_key: String,
    pub dev: bool,
}

pub struct Bot {
    commands: Arc<Mutex<Vec<Box<dyn Commands + Send + Sync>>>>,
    pub(crate) torn_api: Arc<Mutex<TornAPI>>,
    secrets: Secrets,
    // For always running functions, they have to be non-blocking otherwise the bot will get stuck on them
    triggers: Vec<Box<dyn Fn(Context, Ready) + Send + Sync>>,
}

impl Bot {
    pub async fn new(secrets: Secrets, torn_api: TornAPI) -> Self {
        let torn_api = Arc::new(Mutex::new(torn_api));
        let commands: Arc<Mutex<Vec<Box<dyn Commands + Send + Sync>>>> =
            Arc::new(Mutex::new(Vec::new()));

        {
            let mut commands_guard = commands.lock().await;
            commands_guard.push(Box::new(ReviveMe::new(secrets.clone(), torn_api.clone())));
            commands_guard.push(Box::new(Contract::new(secrets.clone(), torn_api.clone())));
            commands_guard.push(Box::new(Stats::new(secrets.clone(), torn_api.clone())));
            commands_guard.push(Box::new(Report::new(secrets.clone(), torn_api.clone())));
            commands_guard.push(Box::new(Help::new(commands.clone(), secrets.clone())));
        }

        Self {
            commands,
            secrets,
            torn_api,
            triggers: Vec::new(),
        }
    }

    pub fn add_trigger(&mut self, trigger: impl Fn(Context, Ready) + Send + Sync + 'static) {
        self.triggers.push(Box::new(trigger));
    }
    pub fn set_secrets(&mut self, secrets: Secrets) {
        self.secrets = secrets;
    }
}

#[async_trait]
impl EventHandler for Bot {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.content == "!hello" {
            if let Err(e) = msg.channel_id.say(&ctx.http, "world!").await {
                error!("Error sending message: {:?}", e);
            }
        }
    }
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        // Clears all the commands from the guild (for cleanup when in development phase)
        let cmds = ctx
            .http
            .get_guild_commands(GuildId::from(self.secrets.revive_faction_guild))
            .await
            .unwrap();

        for cmd in cmds {
            ctx.http
                .delete_guild_command(GuildId::from(self.secrets.revive_faction_guild), cmd.id)
                .await
                .unwrap();
        }

        {
            // Acquire a lock to access the commands
            let commands_guard = self.commands.lock().await;
            for command in commands_guard.iter() {
                let cmd = command.register();

                if command.is_global() {
                    ctx.http.create_global_command(&cmd).await.unwrap();
                }


                ctx.http
                    .create_guild_command(GuildId::from(self.secrets.revive_faction_guild), &cmd)
                    .await
                    .unwrap();


            }
        }

        for trigger in self.triggers.iter() {
            trigger(ctx.clone(), ready.clone());
        }
    }
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let mut commands_guard = self.commands.lock().await;

        // Perform command processing in the current thread to avoid `Send` issues
        for command in commands_guard.iter_mut() {
            if command.authorized(ctx.clone(), interaction.clone()).await {
                command.action(ctx.clone(), interaction.clone()).await;
            }
        }
    }
}

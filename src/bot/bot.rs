use log::info;
use serenity::all::{Context, CreateCommand, EventHandler, GuildId, Message, Ready};
use serenity::model::application::{Command, Interaction};
use shuttle_runtime::async_trait;
use std::cell::RefCell;
use tokio::sync::Mutex;
use tracing::error;

use crate::bot::commands::command::Commands; // Import the trait
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
    pub revive_faction_api_key: String,
    pub test_api_key: String,
    pub dev: bool,
}

pub struct Bot {
    commands: Mutex<Vec<Box<dyn Commands + Send + Sync>>>,
    pub(crate) torn_api: TornAPI,
    secrets: Secrets,
}

impl Bot {
    pub async fn new(secrets: Secrets) -> Self {
        let commands: Mutex<Vec<Box<dyn Commands + Send + Sync>>> = Mutex::new(Vec::new());

        {
            // idk
            let mut commands_guard = commands.lock().await;
            commands_guard.push(Box::new(ReviveMe::new(secrets.clone()))); // Initialize commands async
        }

        Self {
            commands,
            secrets,
            torn_api: TornAPI::new(vec![]),
        }
    }

    pub fn set_secrets(&mut self, secrets: Secrets) {
        self.secrets = secrets;
    }
}

#[async_trait]
impl EventHandler for Bot {
    async fn message(&self, ctx: Context, msg: Message) {
        info!("Message: {:?}", msg.content);
        if msg.content == "!hello" {
            if let Err(e) = msg.channel_id.say(&ctx.http, "world!").await {
                error!("Error sending message: {:?}", e);
            }
        }
    }
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        // Clears all the commands from the guild
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

        let mut commands = vec![];
        {
            // Acquire a lock to access the commands
            let commands_guard = self.commands.lock().await;
            for command in commands_guard.iter() {
                let command = command.register();
                commands.push(command);
            }
        }

        ctx.http
            .create_guild_commands(GuildId::from(self.secrets.revive_faction_guild), &commands)
            .await
            .unwrap();

        // takes up to 1 hour to update global commands
        ctx.http.create_global_commands(&commands).await.unwrap();

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

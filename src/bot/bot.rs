use log::info;
use serenity::all::{Context, CreateCommand, EventHandler, Message, Ready};
use serenity::model::application::{Command, Interaction};
use serenity::async_trait;
use tracing::error;

use crate::bot::commands::command::Commands;  // Import the trait
use crate::bot::commands::reviveme::ReviveMe;
pub struct Bot<>{
    //commands: Vec<Box<dyn Commands>>,
}

impl Bot {
    pub fn new() -> Self {
        Self {
            //commands: Vec::new(),
        }
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
        let commands = vec![ReviveMe::new()];

        for command in commands {
            let command = command.register();
            if let Err(e) = ctx.http.create_global_command(&command).await {
                error!("Error creating command: {:?}", e);
            }
        }

        //self.commands = commands.into_iter().map(|c| Box::new(c) as Box<dyn Commands>).collect();
    }
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {


        match interaction {
            Interaction::Command(ref command) => {
                let command_name = command.data.name.as_str();
                let command = match command_name {
                    "reviveme" => ReviveMe::new(),
                    _ => return,
                };
                command.action(ctx, interaction).await;
            }
            _ => {}
        }

        //info!("Interaction: {:?}", interaction);
    }
}
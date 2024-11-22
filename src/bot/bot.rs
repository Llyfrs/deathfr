use log::info;
use serenity::all::{Context, CreateCommand, EventHandler, Message, Ready};
use serenity::model::application::{Command, Interaction};
use serenity::async_trait;
use tracing::error;

pub struct Bot;

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
        Command::create_global_command(&ctx.http, CreateCommand::new("hello").description("Says hello!"))
            .await
            .expect("Failed to register command");
    }
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        info!("Interaction: {:?}", interaction);
    }
}
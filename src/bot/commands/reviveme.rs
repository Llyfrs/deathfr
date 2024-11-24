use serenity::all::{Content, Context, CreateCommand, CreateInteractionResponse, MessageBuilder};
use crate::bot::commands::command::Commands;
use serenity::model::application::{Interaction};
pub struct ReviveMe;
impl ReviveMe {
    pub fn new() -> Self {
        Self{}
    }
}
impl Commands for ReviveMe {
    fn register(&self) -> CreateCommand {
        CreateCommand::new("reviveme")
            .description("Ask for a revive")
    }
    async fn action(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => {
                command.channel_id.say(&ctx.http, "Revive me!").await.unwrap();
            }
            _ => {}
        }
    }
}


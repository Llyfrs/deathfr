use crate::bot::Bot;
use serenity::all::{CommandInteraction, ComponentInteraction, Context};
use serenity::builder::CreateCommand;
use serenity::model::application::Interaction;
use shuttle_runtime::async_trait;
use std::future::Future;

#[async_trait]
pub(crate) trait Commands: Send + Sync {
    fn register(&self) -> CreateCommand;
    async fn action(&mut self, ctx: Context, interaction: Interaction);
}

pub async fn interaction_command<F, Fut>(interaction: Interaction, function: F)
where
    F: FnOnce(CommandInteraction) -> Fut,
    Fut: Future<Output = ()>,
{
    if let Interaction::Command(command) = interaction {
        function(command).await;
    }
}


pub async fn interaction_button<F, Fut>(interaction: Interaction, function: F)
where
    F: FnOnce(ComponentInteraction) -> Fut,
    Fut: Future<Output = ()>,
{
    if let Interaction::Component(button) = interaction {
        function(button).await;
    }
}
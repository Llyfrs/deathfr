use serenity::all::{Context};
use serenity::builder::CreateCommand;
use serenity::model::application::{Interaction};

pub(crate) trait Commands {
    fn register(&self) -> CreateCommand;
    async fn action(&self, ctx: Context, interaction: Interaction);

}



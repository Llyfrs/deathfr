use serenity::all::{ Context, EmbedField};
use serenity::builder::CreateCommand;
use serenity::model::application::Interaction;
use shuttle_runtime::async_trait;

#[async_trait]
pub(crate) trait Commands: Send + Sync {
    /// Registers the command with the discord api
    fn register(&self) -> CreateCommand;

    /// The main action of the command, this is where the command logic should be placed
    async fn action(&mut self, ctx: Context, interaction: Interaction);

    /// Determines if the command is allowed ot be used by either user or guild, also a good place to handle dev logic
    async fn authorized(&self, _ctx: Context, _interaction: Interaction) -> bool {
        true
    }

    /// Determines if the command is set as global and so accessible from any guild and by users as application command
    fn is_global(&self) -> bool {
        false
    }

    /// Returns field with information about the command
    /// Returns vec to accommodate subcommands
    /// Returns None if no help is available
    fn help(&self) -> Option<Vec<EmbedField>> {
        None
    }
}



use serenity::all::{CommandOptionType, Context, CreateCommand, Interaction};
use serenity::async_trait;
use serenity::builder::CreateCommandOption;
use crate::bot::commands::command::Commands;

pub(crate) struct Report;



impl Report {
    pub fn new() -> Self {
        Report
    }
}


#[async_trait]
impl Commands for Report {
    fn register(&self) -> CreateCommand {
        CreateCommand::new("report").description("Generate contract report").add_option(
            CreateCommandOption::new(CommandOptionType::String, "contract_id", "The contract ID of the player you want to report").required(true)
        )
    }

    async fn action(&mut self, ctx: Context, interaction: Interaction) {

    }


    /// Everybody should have access to this, the passed contract id will be used to generate a report
    fn is_global(&self) -> bool {
        true
    }

}
use std::os::linux::raw::stat;
use std::sync::{Arc};
use tokio::sync::Mutex;
use serenity::all::{Context, CreateCommand, Interaction};
use serenity::async_trait;
use crate::bot::commands::command::Commands;
use crate::bot::Secrets;
use crate::torn_api::TornAPI;

pub(crate) struct Stats {
    api : Arc<Mutex<TornAPI>>,
    secrets: Secrets,
}

impl Stats {
    pub fn new(secrets: Secrets, api: Arc<Mutex<TornAPI>>) -> Self {
        Self {
            api,
            secrets,
        }
    }
}


#[async_trait]
impl Commands for Stats {
    fn register(&self) -> CreateCommand {
        CreateCommand::new("stats").description("Get your personal reviving stats")
    }

    async fn action(&mut self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => {
                // This could be an authorized function??
                if command.data.name.as_str() != "stats" {
                    return;
                }


            }
            _ => {}
        }
    }

}
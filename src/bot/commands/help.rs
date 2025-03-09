use crate::bot::commands::command::Commands;
use crate::bot::Secrets;
use serenity::all::{
    Context, CreateCommand, CreateEmbed, CreateEmbedFooter, CreateInteractionResponse,
    CreateInteractionResponseMessage, EmbedField, Interaction,
};
use serenity::async_trait;
use std::sync::Arc;
use std::vec;
use tokio::sync::Mutex;

pub struct Help {
    pub commands: Arc<Mutex<Vec<Box<dyn Commands + Send + Sync>>>>,
    pub secrets: Secrets,
}
impl Help {
    pub fn new(
        commands: Arc<Mutex<Vec<Box<dyn Commands + Send + Sync>>>>,
        secrets: Secrets,
    ) -> Self {
        Self { commands, secrets }
    }
}

#[async_trait]
impl Commands for Help {
    fn register(&self) -> CreateCommand {
        CreateCommand::new("help").description("Get a list of all available commands")
    }

    async fn action(&mut self, ctx: Context, interaction: Interaction) {
        let commands = self.commands.clone();
        let secrets = self.secrets.clone();

        // Avoids deadlock ??
        tokio::spawn(async move {
            get_help(commands, ctx.clone(), interaction.clone(), secrets).await;
        });
    }

    fn is_global(&self) -> bool {
        true
    }

    fn help(&self) -> Option<Vec<EmbedField>> {
        Some(vec![EmbedField::new(
            "/help",
            "Get a list of all available commands",
            false,
        )])
    }
}

async fn get_help(
    commands: Arc<Mutex<Vec<Box<dyn Commands + Send + Sync>>>>,
    ctx: Context,
    interaction: Interaction,
    secrets: Secrets,
) {
    match interaction {
        Interaction::Command(ref command) => {
            if command.data.name.as_str() != "help" {
                return;
            }

            let mut fields = Vec::new();

            for command in commands.lock().await.iter() {
                if command.authorized(ctx.clone(), interaction.clone()).await {
                    if let Some(field) = command.help() {
                        for field in field {
                            fields.push((field.name, field.value, field.inline));
                        }
                    }
                }
            }

            command
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new().embed(
                            CreateEmbed::default()
                                .title("Help")
                                .description("List of all commands **available to you**.")
                                .fields(fields)
                                .footer(CreateEmbedFooter::new("Bot author: Llyfr [2531272]")),
                        ),
                    ),
                )
                .await
                .expect("Failed to create response");
        }
        _ => {}
    }
}

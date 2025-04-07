use serenity::all::{CommandInteraction, Context, CreateActionRow, CreateInteractionResponse, CreateInteractionResponseMessage};

pub async fn create_response(ctx: &Context, command: CommandInteraction, message: String, ephemeral: bool) {
    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(message)
                    .ephemeral(ephemeral),
            ),
        )
        .await
        .unwrap();
}
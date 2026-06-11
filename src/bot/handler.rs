use serenity::all::{ChannelId, FullEvent, Interaction, RoleId};
use tracing::error;

use crate::bot::commands::{contract, reviveme, submitkey};
use crate::bot::data::{Data, Error};

/// Handles everything poise does not route itself: plain messages and
/// component/modal interactions belonging to the commands.
pub async fn event_handler(
    ctx: &serenity::all::Context,
    event: &FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        FullEvent::Message { new_message } => {
            // Skip processing if the message is from the bot itself
            if new_message.author.id == ctx.cache.current_user().id {
                return Ok(());
            }

            if new_message
                .mention_roles
                .contains(&RoleId::from(data.secrets.revive_role))
                && new_message.channel_id != ChannelId::from(data.secrets.revive_channel)
            {
                // Suggest using /reviveme command (only shown outside the revive channel)
                let reply = new_message
                    .reply(
                        &ctx.http,
                        "Did you know you can now use the /reviveme command instead? Try it out!",
                    )
                    .await;

                if let Err(e) = reply {
                    error!("Error sending reply: {:?}", e);
                }
            }
        }
        FullEvent::InteractionCreate { interaction } => match interaction {
            Interaction::Component(component) => match component.data.custom_id.as_str() {
                "cancel_revive" => reviveme::handle_cancel_revive(ctx, data, component).await?,
                "claim" => reviveme::handle_claim_revive(ctx, data, component).await?,
                "next" | "previous" => contract::handle_pagination(ctx, data, component).await?,
                "submitkey_open_modal" => submitkey::handle_open_modal(ctx, data, component).await?,
                _ => {}
            },
            Interaction::Modal(modal) if modal.data.custom_id == "submitkey_modal" => {
                submitkey::handle_modal_submit(ctx, data, modal).await?
            }
            _ => {}
        },
        _ => {}
    }

    Ok(())
}

use serenity::all::{FullEvent, Interaction};

use crate::bot::commands::{contract, contract_wizard, reviveme, submitkey};
use crate::bot::data::{Data, Error};

/// Handles everything poise does not route itself: component/modal interactions
/// belonging to the commands.
pub async fn event_handler(
    ctx: &serenity::all::Context,
    event: &FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        FullEvent::InteractionCreate { interaction } => match interaction {
            Interaction::Component(component) => {
                let custom_id = component.data.custom_id.as_str();
                if custom_id.starts_with("contract_wizard_") {
                    contract_wizard::handle_component(ctx, data, component).await?;
                } else {
                    match custom_id {
                        "cancel_revive" => {
                            reviveme::handle_cancel_revive(ctx, data, component).await?
                        }
                        "claim" => reviveme::handle_claim_revive(ctx, data, component).await?,
                        "next" | "previous" => {
                            contract::handle_pagination(ctx, data, component).await?
                        }
                        "submitkey_open_modal" => {
                            submitkey::handle_open_modal(ctx, data, component).await?
                        }
                        _ => {}
                    }
                }
            }
            Interaction::Modal(modal) if modal.data.custom_id == "contract_wizard_modal" => {
                contract_wizard::handle_modal(ctx, data, modal).await?
            }
            Interaction::Modal(modal) if modal.data.custom_id == "submitkey_modal" => {
                submitkey::handle_modal_submit(ctx, data, modal).await?
            }
            _ => {}
        },
        _ => {}
    }

    Ok(())
}

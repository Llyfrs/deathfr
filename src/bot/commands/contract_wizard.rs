use crate::bot::auth::{level_of, AccessLevel};
use crate::bot::data::{Context, Data, Error};
use crate::database::structures::{Contract, Status};
use crate::database::Database;
use crate::pricing::PricingType;
use chrono::{DateTime, NaiveDateTime, Utc};
use mongodb::bson::doc;
use poise::CreateReply;
use rand::distr::Alphanumeric;
use rand::Rng;
use serenity::all::{
    ActionRowComponent, ButtonStyle, ComponentInteraction, CreateActionRow, CreateButton,
    CreateEmbed, CreateInputText, CreateInteractionResponse, CreateInteractionResponseFollowup,
    CreateInteractionResponseMessage, CreateModal, InputTextStyle,
    ModalInteraction, UserId,
};
use serenity::builder::CreateActionRow as CreateActionRowBuilder;
use serenity::utils::MessageBuilder;

const MODAL_ID: &str = "contract_wizard_modal";
const INPUT_ID: &str = "contract_wizard_input";
const START_TIME_FORMAT: &str = "YYYY-MM-DD HH:MM";
const START_TIME_FORMAT_HINT: &str = "Format: `YYYY-MM-DD HH:MM` (UTC). Example: `2026-06-20 14:30`.";

/// In-memory wizard state keyed by the ephemeral wizard message id.
#[derive(Clone)]
pub struct ContractWizardState {
    pub user_id: UserId,
    pub step: WizardStep,
    pub contract_name: Option<String>,
    pub faction_id: Option<u64>,
    pub faction_name: Option<String>,
    pub min_chance: Option<u64>,
    pub pricing_type: Option<PricingType>,
    /// Explicit cut; `None` at insert uses the pricing-type default.
    pub faction_cut: Option<u64>,
    /// User completed step 5 via skip or custom cut (used for Keep current after back).
    pub faction_cut_set: bool,
    /// Scheduled start; `None` at insert means start immediately.
    pub scheduled_start: Option<DateTime<Utc>>,
    /// User completed step 6 via skip or schedule (used for Keep current after back).
    pub start_time_set: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardStep {
    ContractName,
    FactionId,
    MinChance,
    PricingType,
    FactionCut,
    StartTime,
    Confirm,
}

impl WizardStep {
    fn number(self) -> u8 {
        match self {
            Self::ContractName => 1,
            Self::FactionId => 2,
            Self::MinChance => 3,
            Self::PricingType => 4,
            Self::FactionCut => 5,
            Self::StartTime => 6,
            Self::Confirm => 7,
        }
    }

    fn next(self) -> Option<Self> {
        match self {
            Self::ContractName => Some(Self::FactionId),
            Self::FactionId => Some(Self::MinChance),
            Self::MinChance => Some(Self::PricingType),
            Self::PricingType => Some(Self::FactionCut),
            Self::FactionCut => Some(Self::StartTime),
            Self::StartTime => Some(Self::Confirm),
            Self::Confirm => None,
        }
    }

    fn prev(self) -> Option<Self> {
        match self {
            Self::ContractName => None,
            Self::FactionId => Some(Self::ContractName),
            Self::MinChance => Some(Self::FactionId),
            Self::PricingType => Some(Self::MinChance),
            Self::FactionCut => Some(Self::PricingType),
            Self::StartTime => Some(Self::FactionCut),
            Self::Confirm => Some(Self::StartTime),
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::ContractName => "Contract name",
            Self::FactionId => "Faction ID",
            Self::MinChance => "Minimum revive chance",
            Self::PricingType => "Pricing tier",
            Self::FactionCut => "Faction cut",
            Self::StartTime => "Start time",
            Self::Confirm => "Confirm",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::ContractName => {
                "A short identifier shown in the contract list. Something meaningful like the served faction name and date works well."
            }
            Self::FactionId => {
                "The Torn faction ID to track revives for. If both defense and offensive revives are provided, create two separate contracts."
            }
            Self::MinChance => {
                "Failed revives at or above this success chance percentage count toward payment."
            }
            Self::PricingType => {
                "**External** — $1,000,000 per successful revive, $750,000 per counted failed revive (default 10% faction cut).\n\
                 **Inter Alliance** — $800,000 / $550,000 (default 0% faction cut)."
            }
            Self::FactionCut => {
                "Percentage markup applied on top of the base revive cost. You can accept the pricing-tier default or set a custom percentage."
            }
            Self::StartTime => {
                "When the contract becomes active. Starting immediately creates an **active** contract; a future time creates a **pending** contract until then.\n\n\
                 Use **`YYYY-MM-DD HH:MM`** in **UTC** when scheduling (e.g. `2026-06-20 14:30`)."
            }
            Self::Confirm => {
                "Review the resolved values below. When everything looks correct, create the contract."
            }
        }
    }
}

/// Experimental step-by-step contract creation wizard.
#[poise::command(slash_command, rename = "start-contract-interactive")]
pub async fn start_contract_interactive(ctx: Context<'_>) -> Result<(), Error> {
    if !ensure_admin(&ctx).await? {
        return Ok(());
    }

    let state = ContractWizardState {
        user_id: ctx.author().id,
        step: WizardStep::ContractName,
        contract_name: None,
        faction_id: None,
        faction_name: None,
        min_chance: None,
        pricing_type: None,
        faction_cut: None,
        faction_cut_set: false,
        scheduled_start: None,
        start_time_set: false,
        error: None,
    };

    let (content, embed, components) = render_step(&state);

    let handle = ctx
        .send(
            CreateReply::default()
                .content(content)
                .embed(embed)
                .components(components)
                .ephemeral(true),
        )
        .await?;

    let message = handle.message().await?;

    ctx.data().contract_wizards.lock().await.insert(
        message.id,
        ContractWizardState {
            user_id: ctx.author().id,
            ..state
        },
    );

    Ok(())
}

async fn ensure_admin(ctx: &Context<'_>) -> Result<bool, Error> {
    if level_of(ctx) >= AccessLevel::Admin {
        return Ok(true);
    }

    ctx.send(
        CreateReply::default()
            .content("You are not authorized to use this command.")
            .ephemeral(true),
    )
    .await?;

    Ok(false)
}

pub async fn handle_component(
    ctx: &serenity::all::Context,
    data: &Data,
    component: &ComponentInteraction,
) -> Result<(), Error> {
    let custom_id = component.data.custom_id.as_str();

    let next_state = {
        let mut wizards = data.contract_wizards.lock().await;
        let Some(state) = wizards.get_mut(&component.message.id) else {
            component.defer(&ctx.http).await?;
            return Ok(());
        };

        if component.user.id != state.user_id {
            component.defer(&ctx.http).await?;
            return Ok(());
        }

        match custom_id {
            "contract_wizard_cancel" => {
                wizards.remove(&component.message.id);
                drop(wizards);
                component
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::UpdateMessage(
                            CreateInteractionResponseMessage::new()
                                .content("Contract creation cancelled.")
                                .components(vec![]),
                        ),
                    )
                    .await?;
                return Ok(());
            }
            "contract_wizard_back" => {
                if let Some(prev) = state.step.prev() {
                    state.step = prev;
                    state.error = None;
                }
                state.clone()
            }
            "contract_wizard_keep_current" => {
                if let Some(next) = state.step.next() {
                    state.step = next;
                    state.error = None;
                }
                state.clone()
            }
            "contract_wizard_skip" => match state.step {
                WizardStep::FactionCut => {
                    state.faction_cut = None;
                    state.faction_cut_set = true;
                    state.step = WizardStep::StartTime;
                    state.error = None;
                    state.clone()
                }
                WizardStep::StartTime => {
                    state.scheduled_start = None;
                    state.start_time_set = true;
                    state.step = WizardStep::Confirm;
                    state.error = None;
                    state.clone()
                }
                _ => {
                    component.defer(&ctx.http).await?;
                    return Ok(());
                }
            },
            "contract_wizard_pick_50" => {
                state.min_chance = Some(50);
                state.step = WizardStep::PricingType;
                state.error = None;
                state.clone()
            }
            "contract_wizard_pick_75" => {
                state.min_chance = Some(75);
                state.step = WizardStep::PricingType;
                state.error = None;
                state.clone()
            }
            "contract_wizard_pick_90" => {
                state.min_chance = Some(90);
                state.step = WizardStep::PricingType;
                state.error = None;
                state.clone()
            }
            "contract_wizard_pick_external" => {
                state.pricing_type = Some(PricingType::External);
                state.step = WizardStep::FactionCut;
                state.error = None;
                state.clone()
            }
            "contract_wizard_pick_inter_alliance" => {
                state.pricing_type = Some(PricingType::InterAlliance);
                state.step = WizardStep::FactionCut;
                state.error = None;
                state.clone()
            }
            "contract_wizard_open_modal" | "contract_wizard_change" => {
                drop(wizards);
                return open_modal_for_step(ctx, data, component, custom_id).await;
            }
            "contract_wizard_pick_custom_chance"
            | "contract_wizard_pick_custom_cut"
            | "contract_wizard_pick_schedule" => {
                drop(wizards);
                return open_modal_for_step(ctx, data, component, custom_id).await;
            }
            "contract_wizard_confirm" => {
                let snapshot = state.clone();
                drop(wizards);
                return confirm_and_create(ctx, data, component, snapshot).await;
            }
            _ => {
                component.defer(&ctx.http).await?;
                return Ok(());
            }
        }
    };

    let snapshot = next_state.clone();
    {
        let mut wizards = data.contract_wizards.lock().await;
        if let Some(state) = wizards.get_mut(&component.message.id) {
            *state = next_state;
        }
    }

    respond_update(ctx, component, &snapshot).await
}

fn modal_message_id(modal: &ModalInteraction) -> Option<serenity::all::MessageId> {
    modal.message.as_ref().map(|message| message.id)
}

pub async fn handle_modal(
    ctx: &serenity::all::Context,
    data: &Data,
    modal: &ModalInteraction,
) -> Result<(), Error> {
    let input = extract_modal_input(modal);
    let Some(message_id) = modal_message_id(modal) else {
        modal.defer(&ctx.http).await?;
        return Ok(());
    };

    let Some(raw) = input else {
        let mut wizards = data.contract_wizards.lock().await;
        let Some(state) = wizards.get_mut(&message_id) else {
            modal.defer(&ctx.http).await?;
            return Ok(());
        };
        if modal.user.id != state.user_id {
            modal.defer(&ctx.http).await?;
            return Ok(());
        }
        state.error = Some(empty_field_error(state.step));
        let snapshot = state.clone();
        drop(wizards);
        return respond_update_modal(ctx, modal, &snapshot).await;
    };

    let (user_id, step) = {
        let wizards = data.contract_wizards.lock().await;
        let Some(state) = wizards.get(&message_id) else {
            modal.defer(&ctx.http).await?;
            return Ok(());
        };
        if modal.user.id != state.user_id {
            modal.defer(&ctx.http).await?;
            return Ok(());
        }
        (state.user_id, state.step)
    };

    let mut next_state = {
        let wizards = data.contract_wizards.lock().await;
        wizards.get(&message_id).cloned().unwrap()
    };

    next_state.error = None;

    match step {
        WizardStep::ContractName => {
            let name = raw.trim().to_string();
            if name.is_empty() {
                next_state.error = Some(
                    "Contract name cannot be empty — enter a name and try again.".to_string(),
                );
            } else {
                next_state.contract_name = Some(name);
                next_state.step = WizardStep::FactionId;
            }
        }
        WizardStep::FactionId => {
            let Ok(faction_id) = raw.trim().parse::<u64>() else {
                next_state.faction_id = None;
                next_state.faction_name = None;
                next_state.error = Some(
                    "Faction ID must be a valid number — check and try again.".to_string(),
                );
                return finish_modal(ctx, data, modal, message_id, next_state).await;
            };

            let faction_data = match data
                .torn_api
                .lock()
                .await
                .get_faction_data(faction_id)
                .await
            {
                Ok(data) => data,
                Err(e) => {
                    let message = format!("Failed to fetch faction data from Torn: {e:#}");
                    log::info!("{message}");
                    next_state.faction_id = None;
                    next_state.faction_name = None;
                    next_state.error = Some(
                        "Failed to fetch faction data from Torn — please try again later."
                            .to_string(),
                    );
                    return finish_modal(ctx, data, modal, message_id, next_state).await;
                }
            };

            if faction_data.get("error").is_some() {
                next_state.faction_id = None;
                next_state.faction_name = None;
                next_state.error = Some(
                    "Invalid faction ID — check the number and try again.".to_string(),
                );
            } else {
                next_state.faction_id = Some(faction_id);
                next_state.faction_name = faction_data
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                next_state.step = WizardStep::MinChance;
            }
        }
        WizardStep::MinChance => {
            let Ok(min_chance) = raw.trim().parse::<u64>() else {
                next_state.error = Some(
                    "Minimum chance must be a number — enter a value between 0 and 100.".to_string(),
                );
                return finish_modal(ctx, data, modal, message_id, next_state).await;
            };
            if min_chance > 100 {
                next_state.error = Some(
                    "Minimum chance must be between 0 and 100 — try again.".to_string(),
                );
            } else {
                next_state.min_chance = Some(min_chance);
                next_state.step = WizardStep::PricingType;
            }
        }
        WizardStep::FactionCut => {
            let Ok(faction_cut) = raw.trim().parse::<u64>() else {
                next_state.error = Some(
                    "Faction cut must be a valid number — enter a percentage and try again."
                        .to_string(),
                );
                return finish_modal(ctx, data, modal, message_id, next_state).await;
            };
            next_state.faction_cut = Some(faction_cut);
            next_state.faction_cut_set = true;
            next_state.step = WizardStep::StartTime;
        }
        WizardStep::StartTime => match parse_start_time(&raw) {
            Ok(started_at) => {
                next_state.scheduled_start = Some(started_at);
                next_state.start_time_set = true;
                next_state.step = WizardStep::Confirm;
            }
            Err(error) => next_state.error = Some(error),
        },
        _ => {
            modal.defer(&ctx.http).await?;
            return Ok(());
        }
    }

    let _ = user_id;
    finish_modal(ctx, data, modal, message_id, next_state).await
}

async fn finish_modal(
    ctx: &serenity::all::Context,
    data: &Data,
    modal: &ModalInteraction,
    message_id: serenity::all::MessageId,
    next_state: ContractWizardState,
) -> Result<(), Error> {
    {
        let mut wizards = data.contract_wizards.lock().await;
        if let Some(state) = wizards.get_mut(&message_id) {
            *state = next_state.clone();
        }
    }
    respond_update_modal(ctx, modal, &next_state).await
}

async fn confirm_and_create(
    ctx: &serenity::all::Context,
    data: &Data,
    component: &ComponentInteraction,
    state: ContractWizardState,
) -> Result<(), Error> {
    let Some(pricing_type) = state.pricing_type else {
        component.defer(&ctx.http).await?;
        return Ok(());
    };

    let faction_cut = state
        .faction_cut
        .unwrap_or(pricing_type.default_faction_cut() as u64);
    let started_at = state.scheduled_start.unwrap_or_else(Utc::now);
    let status = if started_at > Utc::now() {
        Status::Pending
    } else {
        Status::Active
    };

    let faction_id = state.faction_id.unwrap_or(0);
    let faction_data = match data
        .torn_api
        .lock()
        .await
        .get_faction_data(faction_id)
        .await
    {
        Ok(data) => data,
        Err(e) => {
            let message = format!("Failed to fetch faction data from Torn: {e:#}");
            log::info!("{message}");
            let mut errored = state.clone();
            errored.error = Some(
                "Failed to fetch faction data from Torn — please try again later.".to_string(),
            );
            {
                let mut wizards = data.contract_wizards.lock().await;
                if let Some(wizard) = wizards.get_mut(&component.message.id) {
                    *wizard = errored.clone();
                }
            }
            respond_update(ctx, component, &errored).await?;
            return Ok(());
        }
    };

    if faction_data.get("error").is_some() {
        let mut errored = state.clone();
        errored.error = Some(
            "Invalid faction ID — check the number and try again.".to_string(),
        );
        errored.step = WizardStep::FactionId;
        {
            let mut wizards = data.contract_wizards.lock().await;
            if let Some(s) = wizards.get_mut(&component.message.id) {
                *s = errored.clone();
            }
        }
        return respond_update(ctx, component, &errored).await;
    }

    let contract = Contract {
        id: None,
        contract_id: generate_contract_id().await,
        contract_name: state.contract_name.clone().unwrap_or_default(),
        faction_id,
        min_chance: state.min_chance.unwrap_or(0),
        started: started_at.timestamp() as u64,
        ended: 0,
        status,
        faction_cut: faction_cut as i64,
        pricing_type,
        revives_synced: false,
    };

    let status_label = match contract.status {
        Status::Active => "active",
        Status::Pending => "pending",
        Status::Ended => "ended",
    };

    let contract_id = contract.contract_id.clone();

    let message = MessageBuilder::new()
        .push("Contract created with ID: ")
        .push_mono(&contract_id)
        .push(" at ")
        .push(format_timestamp(contract.started))
        .push(" and is ")
        .push(status_label)
        .push(" (pricing: ")
        .push(contract.pricing_type.label())
        .push(").")
        .build();

    Database::insert(contract).await.unwrap();

    data.contract_wizards.lock().await.remove(&component.message.id);

    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content(message)
                    .components(vec![]),
            ),
        )
        .await?;

    let id_notice = MessageBuilder::new()
        .push("**Your contract ID:** ")
        .push_mono(&contract_id)
        .push("\n\nSave this ID — you need it for `/contract end` and for the contracted faction to run `/report`.")
        .build();

    component
        .create_followup(
            &ctx.http,
            CreateInteractionResponseFollowup::new()
                .content(id_notice)
                .ephemeral(true),
        )
        .await?;

    Ok(())
}

async fn open_modal_for_step(
    ctx: &serenity::all::Context,
    data: &Data,
    component: &ComponentInteraction,
    custom_id: &str,
) -> Result<(), Error> {
    let step = {
        let wizards = data.contract_wizards.lock().await;
        let Some(state) = wizards.get(&component.message.id) else {
            component.defer(&ctx.http).await?;
            return Ok(());
        };
        if component.user.id != state.user_id {
            component.defer(&ctx.http).await?;
            return Ok(());
        }
        state.step
    };

    let (title, label, placeholder, style) = match custom_id {
        "contract_wizard_pick_custom_chance" => (
            "Custom minimum chance",
            "Minimum chance (%)",
            "e.g. 85",
            InputTextStyle::Short,
        ),
        "contract_wizard_pick_custom_cut" => (
            "Custom faction cut",
            "Faction cut (%)",
            "e.g. 15",
            InputTextStyle::Short,
        ),
        "contract_wizard_pick_schedule" => (
            "Schedule contract start (UTC)",
            START_TIME_FORMAT,
            "2026-06-20 14:30",
            InputTextStyle::Short,
        ),
        "contract_wizard_change" | "contract_wizard_open_modal" => match step {
            WizardStep::FactionId => (
                "Faction ID",
                "Faction ID",
                "e.g. 12345",
                InputTextStyle::Short,
            ),
            _ => (
                "Contract name",
                "Contract name",
                "e.g. Acme Corp March 2026",
                InputTextStyle::Short,
            ),
        },
        _ => {
            component.defer(&ctx.http).await?;
            return Ok(());
        }
    };

    let modal = CreateModal::new(MODAL_ID, title).components(vec![CreateActionRowBuilder::InputText(
        CreateInputText::new(style, label, INPUT_ID).placeholder(placeholder).required(true),
    )]);

    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;

    Ok(())
}

fn extract_modal_input(modal: &ModalInteraction) -> Option<String> {
    for row in &modal.data.components {
        for comp in &row.components {
            if let ActionRowComponent::InputText(input) = comp {
                if input.custom_id == INPUT_ID {
                    return input.value.as_ref().map(|v| v.trim().to_string());
                }
            }
        }
    }
    None
}

async fn respond_update(
    ctx: &serenity::all::Context,
    component: &ComponentInteraction,
    state: &ContractWizardState,
) -> Result<(), Error> {
    let error = state.error.clone();
    let (content, embed, components) = render_step(state);
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .embed(embed)
                    .components(components),
            ),
        )
        .await?;
    if let Some(error) = error {
        send_error_followup_component(ctx, component, &error).await?;
    }
    Ok(())
}

async fn respond_update_modal(
    ctx: &serenity::all::Context,
    modal: &ModalInteraction,
    state: &ContractWizardState,
) -> Result<(), Error> {
    let error = state.error.clone();
    let (content, embed, components) = render_step(state);
    modal
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .embed(embed)
                    .components(components),
            ),
        )
        .await?;
    if let Some(error) = error {
        send_error_followup_modal(ctx, modal, &error).await?;
    }
    Ok(())
}

fn error_followup_content(error: &str) -> String {
    format!("**Contract wizard error**\n{error}")
}

async fn send_error_followup_modal(
    ctx: &serenity::all::Context,
    modal: &ModalInteraction,
    error: &str,
) -> Result<(), Error> {
    modal
        .create_followup(
            &ctx.http,
            CreateInteractionResponseFollowup::new()
                .content(error_followup_content(error))
                .ephemeral(true),
        )
        .await?;
    Ok(())
}

async fn send_error_followup_component(
    ctx: &serenity::all::Context,
    component: &ComponentInteraction,
    error: &str,
) -> Result<(), Error> {
    component
        .create_followup(
            &ctx.http,
            CreateInteractionResponseFollowup::new()
                .content(error_followup_content(error))
                .ephemeral(true),
        )
        .await?;
    Ok(())
}

fn render_step(state: &ContractWizardState) -> (String, CreateEmbed, Vec<CreateActionRow>) {
    let step = state.step;
    let mut description = step.description().to_string();

    if let Some(error) = &state.error {
        description.push_str("\n\n**Error:** ");
        description.push_str(error);
    }

    if step == WizardStep::StartTime {
        description.push_str("\n\n");
        description.push_str(START_TIME_FORMAT_HINT);
        if state.scheduled_start.is_none() {
            let now = Utc::now().timestamp();
            description.push_str(&format!(
                "\n\nSkipping starts the contract immediately ({})",
                format_timestamp(now as u64)
            ));
        }
    }

    let mut embed = CreateEmbed::new()
        .title(format!(
            "Create Contract (step {}/7) — {}",
            step.number(),
            step.title()
        ))
        .description(description)
        .field("Progress", build_progress(state), false);

    if step == WizardStep::StartTime {
        embed = embed.field("Time format (UTC)", START_TIME_FORMAT, false);
    }

    let mut rows: Vec<Vec<CreateButton>> = vec![];

    match step {
        WizardStep::ContractName => {
            if state.contract_name.is_some() && state.error.is_none() {
                rows.push(vec![
                    nav_button("contract_wizard_keep_current", "Keep current", ButtonStyle::Success),
                    nav_button("contract_wizard_change", "Change", ButtonStyle::Primary),
                ]);
            } else {
                rows.push(vec![nav_button(
                    "contract_wizard_open_modal",
                    "Enter name",
                    ButtonStyle::Primary,
                )]);
            }
        }
        WizardStep::FactionId => {
            if state.faction_id.is_some() && state.error.is_none() {
                rows.push(vec![
                    nav_button("contract_wizard_keep_current", "Keep current", ButtonStyle::Success),
                    nav_button("contract_wizard_change", "Change", ButtonStyle::Primary),
                ]);
            } else {
                rows.push(vec![nav_button(
                    "contract_wizard_open_modal",
                    "Enter faction ID",
                    ButtonStyle::Primary,
                )]);
            }
        }
        WizardStep::MinChance => {
            let mut row = vec![
                nav_button("contract_wizard_pick_50", "50%", ButtonStyle::Secondary),
                nav_button("contract_wizard_pick_75", "75%", ButtonStyle::Secondary),
                nav_button("contract_wizard_pick_90", "90%", ButtonStyle::Secondary),
                nav_button(
                    "contract_wizard_pick_custom_chance",
                    "Custom",
                    ButtonStyle::Primary,
                ),
            ];
            if let Some(value) = state.min_chance {
                if state.error.is_none() {
                    let keep_label = if [50, 75, 90].contains(&value) {
                        "Keep current".to_string()
                    } else {
                        format!("Keep ({value}%)")
                    };
                    row.insert(
                        0,
                        nav_button(
                            "contract_wizard_keep_current",
                            keep_label,
                            ButtonStyle::Success,
                        ),
                    );
                }
            }
            rows.push(row);
        }
        WizardStep::PricingType => {
            rows.push(vec![
                nav_button(
                    "contract_wizard_pick_external",
                    "External",
                    ButtonStyle::Primary,
                ),
                nav_button(
                    "contract_wizard_pick_inter_alliance",
                    "Inter Alliance",
                    ButtonStyle::Primary,
                ),
            ]);
        }
        WizardStep::FactionCut => {
            let default_cut = state
                .pricing_type
                .map(|p| p.default_faction_cut())
                .unwrap_or(10);
            let mut row = vec![
                nav_button(
                    "contract_wizard_skip",
                    format!("Skip (default: {default_cut}%)"),
                    ButtonStyle::Secondary,
                ),
                nav_button(
                    "contract_wizard_pick_custom_cut",
                    "Set custom cut",
                    ButtonStyle::Primary,
                ),
            ];
            if state.faction_cut_set && state.error.is_none() {
                let keep_label = match state.faction_cut {
                    Some(cut) => format!("Keep ({cut}%)"),
                    None => format!("Keep default ({default_cut}%)"),
                };
                row.insert(
                    0,
                    nav_button("contract_wizard_keep_current", keep_label, ButtonStyle::Success),
                );
            }
            rows.push(row);
        }
        WizardStep::StartTime => {
            let mut row = vec![
                nav_button(
                    "contract_wizard_skip",
                    "Skip (starts immediately)",
                    ButtonStyle::Secondary,
                ),
                nav_button(
                    "contract_wizard_pick_schedule",
                    "Schedule start",
                    ButtonStyle::Primary,
                ),
            ];
            if state.start_time_set && state.error.is_none() {
                let keep_label = if state.scheduled_start.is_some() {
                    "Keep scheduled".to_string()
                } else {
                    "Keep (starts immediately)".to_string()
                };
                row.insert(
                    0,
                    nav_button("contract_wizard_keep_current", keep_label, ButtonStyle::Success),
                );
            }
            rows.push(row);
        }
        WizardStep::Confirm => {
            rows.push(vec![nav_button(
                "contract_wizard_confirm",
                "Create contract",
                ButtonStyle::Success,
            )]);
        }
    }

    if step != WizardStep::ContractName {
        if let Some(last) = rows.last_mut() {
            if last.len() < 5 {
                last.push(nav_button("contract_wizard_back", "Back", ButtonStyle::Secondary));
            } else {
                rows.push(vec![nav_button(
                    "contract_wizard_back",
                    "Back",
                    ButtonStyle::Secondary,
                )]);
            }
        }
    }

    if let Some(last) = rows.last_mut() {
        if last.len() < 5 {
            last.push(nav_button("contract_wizard_cancel", "Cancel", ButtonStyle::Danger));
        } else {
            rows.push(vec![nav_button(
                "contract_wizard_cancel",
                "Cancel",
                ButtonStyle::Danger,
            )]);
        }
    } else {
        rows.push(vec![nav_button(
            "contract_wizard_cancel",
            "Cancel",
            ButtonStyle::Danger,
        )]);
    }

    let components = rows
        .into_iter()
        .map(|buttons| CreateActionRow::Buttons(buttons))
        .collect();

    (step_content(state), embed, components)
}

fn step_content(state: &ContractWizardState) -> String {
    if let Some(error) = &state.error {
        format!("Interactive contract creation — **{error}**")
    } else {
        "Interactive contract creation".to_string()
    }
}

fn nav_button(id: impl Into<String>, label: impl Into<String>, style: ButtonStyle) -> CreateButton {
    CreateButton::new(id).label(label).style(style)
}

fn build_progress(state: &ContractWizardState) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "**Name:** {}",
        state
            .contract_name
            .as_deref()
            .unwrap_or("*Not set yet*")
    ));

    lines.push(match (state.faction_id, state.faction_name.as_deref()) {
        (Some(id), Some(name)) => format!("**Faction:** {name} ({id})"),
        (Some(id), None) => format!("**Faction:** {id}"),
        _ => "**Faction:** *Not set yet*".to_string(),
    });

    lines.push(format!(
        "**Min chance:** {}",
        state
            .min_chance
            .map(|v| format!("{v}%"))
            .unwrap_or_else(|| "*Not set yet*".to_string())
    ));

    lines.push(format!(
        "**Pricing:** {}",
        state
            .pricing_type
            .map(|p| match p {
                PricingType::External => "External ($1M / $750k)".to_string(),
                PricingType::InterAlliance => "Inter Alliance ($800k / $550k)".to_string(),
                PricingType::Legacy => "Legacy".to_string(),
            })
            .unwrap_or_else(|| "*Not set yet*".to_string())
    ));

    let default_cut = state
        .pricing_type
        .map(|p| p.default_faction_cut())
        .unwrap_or(10);
    lines.push(match state.faction_cut {
        Some(cut) => format!("**Faction cut:** {cut}%"),
        None if state.faction_cut_set
            || matches!(state.step, WizardStep::StartTime | WizardStep::Confirm) =>
        {
            format!("**Faction cut:** Default ({default_cut}%)")
        }
        None if state.pricing_type.is_some() => {
            format!("**Faction cut:** *Not set yet (default: {default_cut}%)*")
        }
        None => "**Faction cut:** *Not set yet*".to_string(),
    });

    let now = Utc::now().timestamp();
    lines.push(match state.scheduled_start {
        Some(dt) => format!("**Start:** {}", format_timestamp(dt.timestamp() as u64)),
        None if state.start_time_set || state.step == WizardStep::Confirm => format!(
            "**Start:** Immediately ({})",
            format_timestamp(now as u64)
        ),
        None if state.pricing_type.is_some() => format!(
            "**Start:** *Not set yet (default: immediately — {})*",
            format_timestamp(now as u64)
        ),
        None => "**Start:** *Not set yet*".to_string(),
    });

    lines.join("\n")
}

fn parse_start_time(start_time: &str) -> Result<DateTime<Utc>, String> {
    let parsed = NaiveDateTime::parse_from_str(start_time.trim(), "%Y-%m-%d %H:%M").map_err(|_| {
        format!(
            "Invalid start time — use {START_TIME_FORMAT} in UTC and try again."
        )
    })?;

    Ok(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc))
}

fn format_timestamp(unix: u64) -> String {
    format!("<t:{unix}:f>")
}

fn empty_field_error(step: WizardStep) -> String {
    match step {
        WizardStep::ContractName => {
            "Contract name cannot be empty — enter a name and try again.".to_string()
        }
        WizardStep::FactionId => {
            "Faction ID cannot be empty — enter a valid number and try again.".to_string()
        }
        WizardStep::MinChance => {
            "Minimum chance cannot be empty — enter a value between 0 and 100.".to_string()
        }
        WizardStep::FactionCut => {
            "Faction cut cannot be empty — enter a percentage and try again.".to_string()
        }
        WizardStep::StartTime => {
            format!("Start time cannot be empty — use {START_TIME_FORMAT} in UTC.").to_string()
        }
        _ => "Please enter a value and try again.".to_string(),
    }
}

async fn generate_contract_id() -> String {
    loop {
        let contract_id: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(6)
            .map(|c| c as char)
            .collect();

        let result: Vec<Contract> =
            Database::get_collection_with_filter(Some(doc! {"contract_id": contract_id.clone()}))
                .await
                .unwrap();

        if result.is_empty() {
            return contract_id;
        }
    }
}

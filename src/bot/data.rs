use std::collections::HashMap;
use std::sync::Arc;

use serenity::all::{CommandInteraction, Message, MessageId, UserId};
use tokio::sync::Mutex;

use crate::bot::commands::contract::ListMessageInfo;
use crate::torn_api::{ReviveMonitor, ReviveSourceConfig, TornAPI};

//**
//  Holds all the required secrets for the bot to work
// *
#[derive(Debug, Clone)]
pub struct Secrets {
    pub revive_channel: u64,
    pub revive_role: u64,
    pub revive_faction_guilds: Vec<u64>,
    pub revive_faction: u64,
    pub owner_id: u64,
    pub admins: Vec<u64>,
    pub revive_faction_api_key: String,
    pub revive_sources: Vec<ReviveSourceConfig>,
    pub test_api_key: String,
    pub dev: bool,
}

impl Secrets {
    pub fn is_revive_faction_guild(&self, guild_id: u64) -> bool {
        self.revive_faction_guilds.contains(&guild_id)
    }
}

/// Shared state passed to every poise command and event handler via the framework context.
pub struct Data {
    pub secrets: Secrets,
    pub torn_api: Arc<Mutex<TornAPI>>,
    pub revive_monitor: Arc<ReviveMonitor>,
    /// Map of messages sent to the reviver channel, keyed by the user that asked for a revive
    pub revive_responses: Mutex<HashMap<UserId, Message>>,
    /// Map of the original /reviveme interactions, keyed by the reviver-channel message id
    pub revive_cancellations: Mutex<HashMap<MessageId, CommandInteraction>>,
    /// Pagination state for /contract list messages
    pub contract_pages: Mutex<HashMap<MessageId, ListMessageInfo>>,
}

impl Data {
    pub fn new(secrets: Secrets, torn_api: TornAPI, revive_monitor: Arc<ReviveMonitor>) -> Self {
        Self {
            secrets,
            torn_api: Arc::new(Mutex::new(torn_api)),
            revive_monitor,
            revive_responses: Mutex::new(HashMap::new()),
            revive_cancellations: Mutex::new(HashMap::new()),
            contract_pages: Mutex::new(HashMap::new()),
        }
    }
}

pub type Error = anyhow::Error;
pub type Context<'a> = poise::Context<'a, Data, Error>;

use std::sync::Arc;

use mongodb::bson::doc;
use tokio::sync::Mutex;

use crate::database::structures::Contract;
use crate::database::Database;
use crate::domain::{Initiator, ReviveTarget};
use crate::torn_api::TornAPI;

/// Errors that can occur during revive execution.
#[derive(Debug)]
pub enum ReviveError {
    ApiError(String),
    DatabaseError(String),
    PlayerNotFound,
}

impl std::fmt::Display for ReviveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReviveError::ApiError(msg) => write!(f, "API error: {}", msg),
            ReviveError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            ReviveError::PlayerNotFound => write!(f, "Player not found"),
        }
    }
}

/// The successful result of a revive request — contains everything the caller
/// (Discord adapter, API handler, etc.) needs to act on the request.
#[derive(Debug, Clone)]
pub struct ReviveResult {
    pub request_id: String,
    pub torn_player_id: u64,
    pub name: String,
    pub faction_id: u64,
    pub faction_name: String,
    pub is_in_contract: bool,
    pub contract_min_chance: Option<u64>,
}

/// Protocol-agnostic revive executor.
///
/// Handles fetching player data and checking faction contracts so that
/// Discord (or any future caller) only needs to handle presentation.
pub struct ReviveExecutor;

impl ReviveExecutor {
    /// Request a revive for the given target.
    ///
    /// If `target.name` is `None`, player data is fetched from the Torn API.
    /// Contract status is always checked against the active contracts collection.
    ///
    /// Returns a `ReviveResult` with all information needed to present the
    /// request (channel message, API response, etc.).
    pub async fn request_revive(
        target: ReviveTarget,
        api: Arc<Mutex<TornAPI>>,
        _initiator: Initiator,
    ) -> Result<ReviveResult, ReviveError> {
        // Resolve player info: use what's provided, or fetch from Torn API.
        let (name, faction_id, faction_name) = if let Some(name) = target.name {
            (name, target.faction_id, target.faction_name.unwrap_or_default())
        } else {
            let mut api_guard = api.lock().await;
            let player = api_guard
                .get_player_data(target.torn_player_id)
                .await
                .map_err(|e| ReviveError::ApiError(e.to_string()))?;

            if player.get("error").is_some() {
                return Err(ReviveError::PlayerNotFound);
            }

            let name = player["name"]
                .as_str()
                .unwrap_or("Unknown")
                .to_string();
            let faction_id = player["faction"]["faction_id"]
                .as_u64()
                .unwrap_or(0);
            let faction_name = player["faction"]["faction_name"]
                .as_str()
                .unwrap_or("")
                .to_string();

            (name, faction_id, faction_name)
        };

        // Check active contracts for the player's faction.
        let contracts: Vec<Contract> = Database::get_collection_with_filter(Some(doc! {
            "faction_id": faction_id as i64,
            "status": "active",
        }))
        .await
        .map_err(|e| ReviveError::DatabaseError(e.to_string()))?;

        let is_in_contract = !contracts.is_empty();
        let contract_min_chance = contracts.first().map(|c| c.min_chance);

        let request_id = bson::oid::ObjectId::new().to_hex();

        Ok(ReviveResult {
            request_id,
            torn_player_id: target.torn_player_id,
            name,
            faction_id,
            faction_name,
            is_in_contract,
            contract_min_chance,
        })
    }
}

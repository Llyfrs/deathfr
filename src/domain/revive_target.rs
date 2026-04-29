use serde::{Deserialize, Serialize};

/// Minimal DTO for requesting a revive — no Discord dependencies.
///
/// In the Discord path, `name`, `faction_id`, and `faction_name` are populated
/// from the existing `Verification` record so the executor can skip the Torn API
/// lookup. For API/programmatic paths, only `torn_player_id` is required and the
/// executor fetches the rest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviveTarget {
    pub torn_player_id: u64,
    pub name: Option<String>,
    pub faction_id: u64,
    pub faction_name: Option<String>,
}

/// Who or what initiated the revive request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Initiator {
    /// Request came through Discord — stores the Discord user ID.
    Discord(u64),
    /// Request came through the programmatic API / CLI.
    Api,
}

impl Initiator {
    /// String representation for logging / DB storage.
    pub fn as_str(&self) -> String {
        match self {
            Initiator::Discord(id) => format!("discord:{}", id),
            Initiator::Api => "api".to_string(),
        }
    }
}

use crate::database::structures::CollectionName;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ReviveEntry {
    pub id: String,
    pub timestamp: u64,
    pub result: String,
    pub chance: f32,
    pub reviver_id: u64,
    pub reviver_faction: u64,
    pub target_id: u64,
    pub target_faction: u64,
    pub target_hospital_reason: String,
    pub target_early_discharge: bool,
    pub target_last_action: TargetLastAction,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TargetLastAction {
    pub status: String,
    pub timestamp: u64,
}

impl CollectionName for ReviveEntry {
    fn collection_name() -> &'static str {
        "revive"
    }
}

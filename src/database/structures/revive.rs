use crate::database::structures::CollectionName;
use crate::database::structures::DatabaseName;
use serde::{Deserialize, Serialize};
use torn_api::models::ReviveSimplified;

impl From<ReviveSimplified> for ReviveEntry {
    fn from(r: ReviveSimplified) -> Self {
        ReviveEntry {
            id: r.id.0.to_string(),
            timestamp: r.timestamp as u64,
            result: r.result,
            chance: r.success_chance as f32,
            reviver_id: r.reviver.id.0 as u64,
            reviver_faction: r.reviver.faction_id.map(|f| f.0 as u64).unwrap_or(0),
            target_id: r.target.id.0 as u64,
            target_faction: r.target.faction_id.map(|f| f.0 as u64).unwrap_or(0),
            target_hospital_reason: r.target.hospital_reason,
            target_early_discharge: r.target.early_discharge,
            target_last_action: TargetLastAction {
                timestamp: r.target.last_action as u64,
                status: r.target.online_status,
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TargetLastAction {
    pub status: String,
    pub timestamp: u64,
}

impl CollectionName for ReviveEntry {
    fn collection_name() -> &'static str {
        "revive"
    }
}

impl DatabaseName for ReviveEntry {}

#[async_trait::async_trait]
impl crate::database::structures::IndexSetup for ReviveEntry {
    async fn ensure_indexes(client: &mongodb::Client) -> mongodb::error::Result<()> {
        let db = client.database(Self::database_name());
        let collection = db.collection::<ReviveEntry>(Self::collection_name());

        let model = mongodb::IndexModel::builder()
            .keys(mongodb::bson::doc! { "id": 1 })
            .options(mongodb::options::IndexOptions::builder().unique(true).build())
            .build();

        collection.create_index(model).await?;
        Ok(())
    }
}

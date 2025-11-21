use crate::database::structures::CollectionName;
use crate::database::structures::DatabaseName;
use serde::{Deserialize, Serialize};

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

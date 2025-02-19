use crate::database::structures::{CollectionName, DatabaseName};
use mongodb::bson;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct APIKey {
    #[serde(rename = "_id")] // Ensure it maps to MongoDB's "_id" field
    pub(crate) id: ObjectId,
    pub(crate) api_key: String,
    pub(crate) discord_id: String,
    pub(crate) last_updated: bson::DateTime,
    pub(crate) name: String,
    pub(crate) torn_id: i32,
    #[serde(default)]
    pub(crate) valid: bool,
}

impl CollectionName for APIKey {
    fn collection_name() -> &'static str {
        "API Keys"
    }
}

impl DatabaseName for APIKey {
    fn database_name() -> &'static str {
        "BigBrother"
    }
}

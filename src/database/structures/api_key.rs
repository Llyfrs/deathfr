use crate::database::structures::{CollectionName, DatabaseName};
use mongodb::bson;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct APIKey {
    #[serde(rename = "_id")] // Ensure it maps to MongoDB's "_id" field
    pub(crate) id: ObjectId,
    #[serde(default)]
    pub(crate) api_key: String,
    #[serde(default)]
    pub(crate) name: String,
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

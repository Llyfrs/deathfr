use crate::database::structures::{CollectionName, DatabaseName};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Status {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "ended")]
    Ended,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Contract {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    // Ensure it maps to MongoDB's "_id" field
    pub(crate) id: Option<ObjectId>,
    pub(crate) contract_id: String,
    pub(crate) contract_name: String,
    pub(crate) faction_id: u64,
    pub(crate) min_chance: u64,
    pub(crate) started: u64,
    pub(crate) ended: u64,
    pub(crate) status: Status,
    pub(crate) faction_cut: i64,
}


impl CollectionName for Contract {
    fn collection_name() -> &'static str {
        "contracts"
    }
}
impl DatabaseName for Contract {
    fn database_name() -> &'static str {
        "deathfr"
    }
}

#[async_trait::async_trait]
impl crate::database::structures::IndexSetup for Contract {
    async fn ensure_indexes(client: &mongodb::Client) -> mongodb::error::Result<()> {
        let db = client.database(Self::database_name());
        let collection = db.collection::<Contract>(Self::collection_name());

        let model = mongodb::IndexModel::builder()
            .keys(mongodb::bson::doc! { "contract_id": 1 })
            .options(mongodb::options::IndexOptions::builder().unique(true).build())
            .build();

        collection.create_index(model).await?;
        Ok(())
    }
}

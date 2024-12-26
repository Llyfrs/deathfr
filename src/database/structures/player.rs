use crate::database::structures::{CollectionName, DatabaseName};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Player {
    #[serde(rename = "_id")]
    pub(crate) id: ObjectId,
    pub(crate) uid: Option<i32>,
    pub(crate) name: Option<String>,
}

impl CollectionName for Player {
    fn collection_name() -> &'static str {
        "Players"
    }
}

impl DatabaseName for Player {
    fn database_name() -> &'static str {
        "BigBrother"
    }
}

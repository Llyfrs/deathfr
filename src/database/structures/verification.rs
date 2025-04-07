use crate::database::structures::{CollectionName, DatabaseName};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Verification {
    pub(crate) torn_player_id: u64,
    pub(crate) discord_id: u64,
    pub(crate) name: String,
    /// Needs to create a database index for this to work
    /// `db.getCollection('verifications').createIndex({expire_at: 1}, {expireAfterSeconds: 0})`
    ///
    /// The idea is to make sure the information like username gets updated now and then in case the user changed it.
    pub(crate) expire_at: DateTime<Utc>,

    pub(crate) faction_id: u64,
    pub(crate) faction_name: String,
}

impl CollectionName for Verification {
    fn collection_name() -> &'static str {
        "verifications"
    }
}

impl DatabaseName for Verification {
    fn database_name() -> &'static str {
        "deathfr"
    }
}

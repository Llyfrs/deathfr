use std::ops::Add;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::database::structures::{CollectionName, DatabaseName};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerCache {

    #[serde(default)]
    pub(crate) user_id: u64, // Defaults to 0 (i32's Default implementation)

    #[serde(default)]
    pub(crate) name: String, // Defaults to "" (String's Default implementation)

    pub(crate) expire_at: DateTime<Utc>,
}

impl Default for PlayerCache {
    fn default() -> Self {
        Self {
            user_id: 0,
            name: String::new(),
            expire_at: Utc::now().add(chrono::Duration::days(7)),
        }
    }
}

impl DatabaseName for PlayerCache {
    fn database_name() -> &'static str {
        "deathfr"
    }
}

impl CollectionName for PlayerCache {
    fn collection_name() -> &'static str {
        "player_cache"
    }
}
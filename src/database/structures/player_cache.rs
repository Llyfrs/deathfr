use crate::database::structures::{CollectionName, DatabaseName};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::ops::Add;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerCache {
    #[serde(default)]
    pub(crate) user_id: u64, // Defaults to 0 (i32's Default implementation)

    #[serde(default)]
    pub(crate) name: String, // Defaults to "" (String's Default implementation)

    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
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

#[async_trait::async_trait]
impl crate::database::structures::IndexSetup for PlayerCache {
    async fn ensure_indexes(client: &mongodb::Client) -> mongodb::error::Result<()> {
        let db = client.database(Self::database_name());
        let collection = db.collection::<PlayerCache>(Self::collection_name());

        // Unique index on user_id
        let unique_model = mongodb::IndexModel::builder()
            .keys(mongodb::bson::doc! { "user_id": 1 })
            .options(mongodb::options::IndexOptions::builder().unique(true).build())
            .build();

        // TTL index on expire_at
        let ttl_model = mongodb::IndexModel::builder()
            .keys(mongodb::bson::doc! { "expire_at": 1 })
            .options(
                mongodb::options::IndexOptions::builder()
                    .expire_after(std::time::Duration::from_secs(0))
                    .build(),
            )
            .build();

        collection.create_index(unique_model).await?;
        collection.create_index(ttl_model).await?;
        Ok(())
    }
}

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
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub(crate) expire_at: DateTime<Utc>,


    /// Note:
    /// faction_id and faction_name will be 0 and ""
    /// for players that aren't in a faction (this is how it is in API)
    /// So there isn't any need for Option<>
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

#[async_trait::async_trait]
impl crate::database::structures::IndexSetup for Verification {
    async fn ensure_indexes(client: &mongodb::Client) -> mongodb::error::Result<()> {
        let db = client.database(Self::database_name());
        let collection = db.collection::<Verification>(Self::collection_name());

        let model = mongodb::IndexModel::builder()
            .keys(mongodb::bson::doc! { "expire_at": 1 })
            .options(
                mongodb::options::IndexOptions::builder()
                    .expire_after(std::time::Duration::from_secs(0))
                    .build(),
            )
            .build();

        collection.create_index(model).await?;
        Ok(())
    }
}

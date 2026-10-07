use crate::database::structures::{CollectionName, DatabaseName};
use serde::{Deserialize, Serialize};

/// Per-guild configuration and state of the live contract channel.
///
/// The channel is owned by the bot: everything posted in it after `configured_after`
/// that is not one of the tracked `messages` gets deleted.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LiveChannel {
    pub guild_id: u64,
    pub channel_id: u64,
    /// Snowflake of the moment the channel was configured. Messages older than this
    /// were in the channel before it became a live channel and are left untouched.
    pub configured_after: u64,
    /// Messages the bot currently shows, in channel order (top to bottom).
    #[serde(default)]
    pub messages: Vec<LiveMessage>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LiveMessage {
    /// What the message shows: `header` or a contract ID.
    pub key: String,
    pub message_id: u64,
    /// Hash of the rendered embed, used to skip edits when nothing changed.
    /// Stored as a string because it does not fit into a BSON Int64.
    pub hash: String,
}

impl CollectionName for LiveChannel {
    fn collection_name() -> &'static str {
        "live_channels"
    }
}

impl DatabaseName for LiveChannel {}

#[async_trait::async_trait]
impl crate::database::structures::IndexSetup for LiveChannel {
    async fn ensure_indexes(client: &mongodb::Client) -> mongodb::error::Result<()> {
        let db = client.database(Self::database_name());
        let collection = db.collection::<LiveChannel>(Self::collection_name());

        let model = mongodb::IndexModel::builder()
            .keys(mongodb::bson::doc! { "guild_id": 1 })
            .options(
                mongodb::options::IndexOptions::builder()
                    .unique(true)
                    .build(),
            )
            .build();

        collection.create_index(model).await?;
        Ok(())
    }
}

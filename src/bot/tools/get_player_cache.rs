use crate::database::structures::PlayerCache;
use crate::database::Database;
use crate::torn_api::TornAPI;
use mongodb::bson::doc;
use std::ops::Add;
use torn_api::models::{UserDiscordPathId, UserId};

/// Cached lookup of a Torn player's display name. Fetches from Torn (and stores a
/// 7-day cache) when the player is not yet in the database.
pub async fn get_player_cache(user_id: u64, api: &TornAPI) -> Option<PlayerCache> {
    let db_result: Vec<PlayerCache> =
        Database::get_collection_with_filter(Some(doc! { "user_id": user_id as i64 }))
            .await
            .unwrap();

    if let Some(existing) = db_result.first() {
        return Some(existing.clone());
    }

    let resp = match api
        .get_player_basic(UserDiscordPathId::UserId(UserId::new(user_id as i32)))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to fetch player data for {user_id}: {e:#}");
            return None;
        }
    };

    let player_cache = PlayerCache {
        user_id,
        name: resp.profile.name,
        expire_at: chrono::Utc::now().add(chrono::Duration::days(7)),
    };
    Database::insert(player_cache.clone()).await.unwrap();
    Some(player_cache)
}
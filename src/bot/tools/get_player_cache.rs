use std::ops::Add;
use mongodb::bson::doc;
use crate::database::Database;
use crate::database::structures::PlayerCache;
use crate::torn_api::TornAPI;

pub async fn get_player_cache(user_id : u64, api: &mut TornAPI) -> Option<PlayerCache> {

    let db_result: Vec<PlayerCache>  = Database::get_collection_with_filter( Some(doc! { "user_id": user_id as i64 }) ).await.unwrap();

    if db_result.is_empty() {
        let player = api.get_player_data(user_id).await.unwrap();
        if player.get("error").is_some() {
            log::error!("Error: {:?}", player.get("error").unwrap());
            return None;
        }
        let name = player["name"].as_str().unwrap().to_string();
        let player_cache = PlayerCache {
            user_id,
            name,
            expire_at: chrono::Utc::now().add(chrono::Duration::days(7)),
        };
        Database::insert(player_cache.clone()).await.unwrap();
        Some(player_cache)
    } else {
        Some(db_result[0].clone())
    }
}
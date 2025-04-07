use std::sync::Arc;
use mongodb::bson::doc;
use tokio::sync::Mutex;
use crate::database::Database;
use crate::database::structures::Verification;
use crate::torn_api::TornAPI;

pub async fn resolve_discord_verification(discord_id: u64, api: Arc<Mutex<TornAPI>>) -> Option<Verification> {

    let filter = doc! { "discord_id": discord_id.to_string() };

    let result = Database::get_collection_with_filter::<Verification>(Some(filter))
        .await
        .unwrap().pop();

    match result {
        None => {

            let player = api.lock().await.get_player_data(discord_id).await.unwrap();

            if let Some(error) = player.get("error") {
                log::info!("Error: {:?}", error);
                return None;
            }

            let verification = Verification {
                torn_player_id: player["player_id"].as_u64().unwrap(),
                discord_id,
                name: player["name"].to_string(),
                expire_at: chrono::Utc::now() + chrono::Duration::days(1),
                faction_id: player["faction"]["faction_id"].as_u64().unwrap(),
                faction_name: player["faction"]["faction_name"].to_string()
            };

            Database::insert(verification.clone()).await.unwrap();


            Some(verification)
        }
        Some(record) => {
            Some(record)
        }
    }

}
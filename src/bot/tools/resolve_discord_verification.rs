use std::sync::Arc;

use mongodb::bson::doc;
use crate::database::Database;
use crate::database::structures::Verification;
use crate::torn_api::TornAPI;
use torn_api::models::{DiscordId, UserDiscordPathId};

/// Resolve a Discord user (by their Discord snowflake id) to a cached [`Verification`],
/// fetching from the Torn API and persisting it when not yet present.
pub async fn resolve_discord_verification(discord_id: u64, api: Arc<TornAPI>) -> Option<Verification> {
    let filter = doc! { "discord_id": discord_id as i64 };

    let result = Database::get_collection_with_filter::<Verification>(Some(filter))
        .await
        .unwrap()
        .pop();

    if let Some(record) = result {
        return Some(record);
    }

    // Not cached — look the Discord user up via Torn and persist the verification.
    let profile = match api
        .get_player_profile(UserDiscordPathId::DiscordId(DiscordId::new(discord_id.to_string())))
        .await
    {
        Ok(resp) => resp.profile,
        Err(e) => {
            log::info!("Failed to fetch player profile for {discord_id}: {e:#}");
            return None;
        }
    };

    // v2 profile only exposes `faction_id`; fetch the faction name separately.
    let (faction_id, faction_name) = match profile.faction_id {
        Some(fid) => match api.get_faction_basic(fid).await {
            Ok(basic) => (fid.0 as u64, basic.basic.name),
            Err(e) => {
                log::info!("Failed to fetch faction {fid:?} for {discord_id}: {e:#}");
                (0, String::new())
            }
        },
        None => (0, String::new()),
    };

    let verification = Verification {
        torn_player_id: profile.id.0 as u64,
        discord_id,
        name: profile.name,
        expire_at: chrono::Utc::now() + chrono::Duration::days(1),
        faction_id,
        faction_name,
    };

    Database::insert(verification.clone()).await.unwrap();

    Some(verification)
}
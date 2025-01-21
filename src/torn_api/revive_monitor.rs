use crate::database::structures::ReviveEntry;
use crate::database::Database;
use crate::torn_api::torn_api::APIKey;
use crate::torn_api::TornAPI;
use chrono::format;
use once_cell::sync::Lazy;
use std::sync::atomic::AtomicBool;
use std::sync::LazyLock;

static UPDATE: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));
pub fn request_update() {
    UPDATE.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Periodically collect revives from Torn API and store them in the database
///
/// Update can be forced by calling `request_update`
///
/// TODO: Recreate log function form samwise (will need to be run from bot with context)
pub async fn revive_monitor(api_key: String, revive_faction: u64) {

    let mut api = TornAPI::new(vec![APIKey {
        key: api_key,
        rate_limit: 2,
        owner: "Piasta Key for Revives".to_string(),
    }]);

    api.set_name("Revive Monitor (Deathfr)".to_string());

    let mut last_revive: u64 = Database::get_value("last_revive").await.unwrap();

    loop {
        let revives = api.get_revives(last_revive).await;

        match revives {
            None => {
                log::error!("Failed to collect revives");
            }
            Some((revives, id)) => {

                // Foolproof faction ID check
                // so that if API owner changes a faction
                // AND get API access we don't collect
                // and more importantly don't update last_revive
                if id != revive_faction {
                    log::error!("Faction ID mismatch, expected: {}, got: {}", revive_faction, id);
                    return;
                }

                let len = revives.len();

                Database::insert_manny(revives.clone())
                    .await
                    .expect("Failed to insert revives");

                last_revive = revives.last().unwrap().timestamp;

                match Database::set_value("last_revive", last_revive).await {
                    Ok(_) => {}
                    Err(_) => {
                        log::error!("Failed to set last revive value");
                        return;
                    }
                }
                log::info!("Collected {} revives, last revive: {}", len, last_revive);
            }
        }

        Database::set_value("last_update", chrono::Utc::now().timestamp())
            .await
            .expect("Failed to set last update value");

        let mut minutes = 0;
        while !UPDATE.load(std::sync::atomic::Ordering::Relaxed) && minutes < 60 {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            minutes += 1;
        }

        UPDATE.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

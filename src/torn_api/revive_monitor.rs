use crate::database::Database;
use crate::torn_api::torn_api::APIKey;
use crate::torn_api::TornAPI;
use tokio::sync::Notify;
use std::sync::LazyLock;

static UPDATE: LazyLock<Notify> = LazyLock::new(|| Notify::new());
pub fn request_update() {
    UPDATE.notify_one();
}

/// Periodically collect revives from Torn API and store them in the database
///
/// Update can be forced by calling `request_update`
///
/// TODO: Recreate log function form samwise (will need to be run from bot with context)
pub async fn revive_monitor(api_key: String, revive_faction: u64) {
    log::info!("Starting revive monitor");

    let mut api = TornAPI::new(vec![APIKey {
        key: api_key,
        rate_limit: 2,
        owner: "Piasta Key for Revives".to_string(),
    }]);

    api.set_name("Revive Monitor (Deathfr)".to_string());

    log::info!("Getting last_revive from database...");
    let mut last_revive: u64 = Database::get_value("last_revive").await.unwrap_or(1);
    log::info!("Got last_revive: {}", last_revive);

    loop {
        let mut sleep_duration = 3600;
        log::info!("Fetching revives from API...");
        let revives = api.get_revives(last_revive).await;
        log::info!("Fetched revives");

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
                    log::error!(
                        "Faction ID mismatch, expected: {}, got: {}",
                        revive_faction,
                        id
                    );
                    return;
                }

                let len = revives.len();
                if len > 900 {
                    sleep_duration = 300;
                }

                if len > 0 {
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
                } else {
                    log::info!("No new revives found.");
                }
            }
        }

        Database::set_value("last_update", chrono::Utc::now().timestamp())
            .await
            .expect("Failed to set last update value");

        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(sleep_duration), UPDATE.notified()).await;
    }
}

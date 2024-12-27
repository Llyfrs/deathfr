use crate::database::structures::ReviveEntry;
use crate::database::Database;
use crate::torn_api::torn_api::APIKey;
use crate::torn_api::TornAPI;
use chrono::format;

pub async fn revive_monitor(api_key: String) {
    let mut api = TornAPI::new(vec![APIKey {
        key: api_key,
        rate_limit: 2,
        owner: "owner".to_string(),
    }]);

    let mut last_revive: u64 = Database::get_value("last_revive").await.unwrap();

    loop {
        let revives = api.get_revives(last_revive).await;

        log::info!("Collecting revives starting from: {}", last_revive);

        match revives {
            None => {}
            Some(revives) => {
                let len = revives.len();

                Database::insert_manny(revives.clone())
                    .await
                    .expect("TODO: panic message");

                last_revive = revives.last().unwrap().timestamp;

                match Database::set_value("last_revive", last_revive).await {
                    Ok (_) => {}
                    Err(_) => {
                        log::error!("Failed to set last revive value");
                        return;
                    }
                }

                log::info!("Collected {} revives, last revive: {}", len, last_revive);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(1800)).await;
    }
}

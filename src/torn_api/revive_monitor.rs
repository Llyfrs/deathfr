use crate::database::Database;
use crate::torn_api::torn_api::APIKey;
use crate::torn_api::TornAPI;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct ReviveSourceConfig {
    pub api_key: String,
    pub faction_ids: Vec<u64>,
}

struct ReviveSource {
    api: Mutex<TornAPI>,
    faction_ids: Vec<u64>,
}

struct SourceSyncResult {
    inserted: usize,
    has_backlog: bool,
}

pub struct SyncResult {
    pub total_inserted: usize,
    pub has_backlog: bool,
}

pub struct ReviveMonitor {
    sources: Vec<ReviveSource>,
    sync_lock: Mutex<()>,
}

impl ReviveMonitor {
    pub fn new(configs: Vec<ReviveSourceConfig>) -> Self {
        let sources = configs
            .into_iter()
            .map(|config| {
                let mut api = TornAPI::new(vec![APIKey {
                    key: config.api_key,
                    rate_limit: 2,
                    owner: "Revive Monitor Key".to_string(),
                }]);
                api.set_name("Revive Monitor (Deathfr)".to_string());
                ReviveSource {
                    api: Mutex::new(api),
                    faction_ids: config.faction_ids,
                }
            })
            .collect();

        Self {
            sources,
            sync_lock: Mutex::new(()),
        }
    }

    fn last_revive_key(faction_id: u64) -> String {
        format!("last_revive_{faction_id}")
    }

    async fn get_last_revive(faction_id: u64) -> u64 {
        Database::get_value(&Self::last_revive_key(faction_id))
            .await
            .unwrap_or(1)
    }

    async fn set_last_revive(faction_id: u64, timestamp: u64) -> anyhow::Result<()> {
        Database::set_value(&Self::last_revive_key(faction_id), timestamp).await?;
        Ok(())
    }

    async fn sync_source(source: &ReviveSource) -> anyhow::Result<SourceSyncResult> {
        let primary_faction = source
            .faction_ids
            .first()
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Revive source has no faction IDs configured"))?;

        let mut last_revive = Self::get_last_revive(primary_faction).await;
        let mut api = source.api.lock().await;
        let revives = api.get_revives(last_revive).await;

        match revives {
            None => {
                log::error!("Failed to collect revives");
                Ok(SourceSyncResult {
                    inserted: 0,
                    has_backlog: false,
                })
            }
            Some((revives, id)) => {
                if !source.faction_ids.contains(&id) {
                    log::error!(
                        "Faction ID mismatch, expected one of {:?}, got {}",
                        source.faction_ids,
                        id
                    );
                    return Ok(SourceSyncResult {
                        inserted: 0,
                        has_backlog: false,
                    });
                }

                let len = revives.len();
                let has_backlog = len > 900;

                if len > 0 {
                    Database::insert_manny(revives.clone()).await?;
                    last_revive = revives.last().unwrap().timestamp;
                    Self::set_last_revive(id, last_revive).await?;
                    log::info!("Collected {len} revives for faction {id}, last revive: {last_revive}");
                } else {
                    log::info!("No new revives found for faction {id}.");
                }

                Ok(SourceSyncResult {
                    inserted: len,
                    has_backlog,
                })
            }
        }
    }

    pub async fn sync_once(&self) -> anyhow::Result<SyncResult> {
        let _guard = self.sync_lock.lock().await;

        let mut total_inserted = 0;
        let mut has_backlog = false;

        for source in &self.sources {
            match Self::sync_source(source).await {
                Ok(result) => {
                    total_inserted += result.inserted;
                    has_backlog |= result.has_backlog;
                }
                Err(e) => {
                    log::error!("Revive source sync failed: {e:#}");
                }
            }
        }

        Database::set_value("last_update", chrono::Utc::now().timestamp()).await?;

        Ok(SyncResult {
            total_inserted,
            has_backlog,
        })
    }

    pub async fn sync_for_contract(&self, _contract_ended: u64) -> anyhow::Result<SyncResult> {
        loop {
            let result = self.sync_once().await?;
            if result.total_inserted == 0 || !result.has_backlog {
                return Ok(result);
            }
        }
    }

    pub async fn run_loop(self: Arc<Self>) {
        log::info!("Starting revive monitor loop");

        loop {
            let sleep_duration = match self.sync_once().await {
                Ok(result) if result.has_backlog => 300,
                Ok(_) => 3600,
                Err(e) => {
                    log::error!("Revive monitor sync failed: {e:#}");
                    3600
                }
            };

            tokio::time::sleep(Duration::from_secs(sleep_duration)).await;
        }
    }
}

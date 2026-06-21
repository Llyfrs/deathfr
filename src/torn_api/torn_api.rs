use crate::database::structures::{ReviveEntry, TargetLastAction};
use log::{error, warn};
use serde_json::Value;
use std::cmp::max;
use tokio::time::{sleep, Duration};

#[derive(Clone)]
pub struct APIKey {
    pub key: String,
    pub rate_limit: u32,
    pub owner: String,
}

#[derive(Clone)]
pub struct TornAPI {
    keys: Vec<APIKey>,
    keys_limits: Vec<APIKey>,
    last_reset: i64,
    key_used: usize,
    name: String,
}

enum TornApiErrorAction {
    RemoveKey,
    Retry(u64),
    Fatal,
}

impl TornAPI {
    pub fn new(keys: Vec<APIKey>) -> TornAPI {
        TornAPI {
            keys: keys.clone(),
            keys_limits: keys.clone(),
            key_used: 0,
            last_reset: chrono::Utc::now().timestamp(),
            name: "TornAPI".to_string(),
        }
    }

    /// Add a new API key to the rotation at runtime
    pub fn add_key(&mut self, key: APIKey) {
        self.keys.push(key.clone());
        self.keys_limits.push(key);
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    fn classify_error_code(code: u64) -> TornApiErrorAction {
        match code {
            // Key-specific errors — remove and retry with another key
            1 | 2 | 10 | 13 | 16 | 18 => TornApiErrorAction::RemoveKey,
            // Transient errors — wait and retry
            5 => TornApiErrorAction::Retry(60),
            8 | 9 => TornApiErrorAction::Retry(30),
            15 | 17 => TornApiErrorAction::Retry(5),
            // Request-level or unknown errors — do not retry or remove keys
            _ => TornApiErrorAction::Fatal,
        }
    }

    fn remove_key(&mut self, key_value: &str, owner: &str, code: u64) {
        warn!(
            "Removing invalid API key for owner {} (Torn API error code {})",
            owner, code
        );
        error!(
            "error invalid key of owner: {} (code {})",
            owner, code
        );

        self.keys.retain(|k| k.key != key_value);
        self.keys_limits.retain(|k| k.key != key_value);

        if !self.keys.is_empty() {
            self.key_used %= self.keys.len();
        } else {
            self.key_used = 0;
        }
    }

    ///
    /// Makes a request to the Torn API using the given base URL (without key).
    /// Handles rate limits by waiting, removes invalid keys and retries with another,
    /// and returns an error for unrecoverable API failures.
    ///
    pub async fn make_request(&mut self, base_url: &str) -> Result<Value, String> {
        loop {
            let key = self.get_key().await?;
            let url = format!(
                "{}&key={}&comment={}",
                base_url, key.key, self.name
            );

            let result = reqwest::get(&url)
                .await
                .map_err(|e| e.to_string())?
                .text()
                .await
                .map_err(|e| e.to_string())?;
            let json: Value =
                serde_json::from_str(&result).map_err(|e| e.to_string())?;

            if let Some(error_obj) = json.get("error").and_then(|e| e.as_object()) {
                let code = error_obj
                    .get("code")
                    .and_then(|c| c.as_u64())
                    .unwrap_or(0);
                let message = error_obj
                    .get("error")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");

                match Self::classify_error_code(code) {
                    TornApiErrorAction::RemoveKey => {
                        self.remove_key(&key.key, &key.owner, code);
                        continue;
                    }
                    TornApiErrorAction::Retry(secs) => {
                        warn!(
                            "Torn API error code {} ({}), retrying in {}s",
                            code, message, secs
                        );
                        sleep(Duration::from_secs(secs)).await;
                        continue;
                    }
                    TornApiErrorAction::Fatal => {
                        return Err(format!(
                            "Torn API error code {}: {}",
                            code, message
                        ));
                    }
                }
            }

            return Ok(json);
        }
    }

    /**
    Gets the next key to use,
    accessing keys using this function makes sure their limits are not exceeded,
    and when they run out of calls, it waits for the reset.
     */
    async fn get_key(&mut self) -> Result<APIKey, String> {
        if self.keys.is_empty() {
            return Err("no valid API keys left".to_string());
        }

        let mut key_to_use = self.key_used % self.keys.len();
        let start_key = key_to_use;

        // If the key to be used is already out of calls we move to the next one,until we find one that can be used.
        while self.keys_limits[key_to_use].rate_limit == 0 {
            self.key_used += 1;
            key_to_use = self.key_used % self.keys.len();

            // If we have checked all keys and all of them are out of calls, we wait for the reset.
            if key_to_use == start_key {
                sleep(Duration::from_secs(max(
                    60 - (chrono::Utc::now().timestamp() - self.last_reset),
                    0,
                ) as u64))
                .await;

                if chrono::Utc::now().timestamp() - self.last_reset >= 60 {
                    self.last_reset = chrono::Utc::now().timestamp();
                    self.key_used = 0;
                    self.keys_limits = self.keys.clone();
                }
            }
        }

        self.key_used += 1;
        self.keys_limits[key_to_use].rate_limit -= 1;

        Ok(self.keys_limits[key_to_use].clone())
    }

    pub async fn get_player_data(&mut self, player_id: u64) -> Result<Value, String> {
        let url = format!(
            "https://api.torn.com/user/{}?selections=profile",
            player_id
        );

        self.make_request(&url).await
    }

    pub async fn get_faction_data(&mut self, faction_id: u64) -> Result<Value, String> {
        let url = format!(
            "https://api.torn.com/faction/{}?selections=basic",
            faction_id
        );

        self.make_request(&url).await
    }

    pub async fn get_revives(&mut self, from: u64) -> Option<(Vec<ReviveEntry>, u64)> {
        let mut revives = Vec::new();

        let url = format!(
            "https://api.torn.com/faction/?selections=revivesfull,basic&from={}",
            from
        );

        let json = self.make_request(&url).await.ok()?;
        let revives_json = json["revives"].as_object()?;
        let faction_id = json["ID"].as_u64()?;

        for (id, data) in revives_json {
            revives.push(ReviveEntry {
                id: id.to_string(),
                timestamp: data.get("timestamp").and_then(|v| v.as_u64()).unwrap_or(0),
                result: data
                    .get("result")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                chance: data.get("chance").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                reviver_id: data.get("reviver_id").and_then(|v| v.as_u64()).unwrap_or(0),
                reviver_faction: data
                    .get("reviver_faction")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                target_id: data.get("target_id").and_then(|v| v.as_u64()).unwrap_or(0),
                target_faction: data
                    .get("target_faction")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                target_hospital_reason: data
                    .get("target_hospital_reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                target_early_discharge: data
                    .get("target_early_discharge")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                target_last_action: TargetLastAction {
                    timestamp: data
                        .get("target_last_action")
                        .and_then(|v| v.get("timestamp"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                    status: data
                        .get("target_last_action")
                        .and_then(|v| v.get("status"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                },
            });
        }

        Some((revives, faction_id))
    }
}

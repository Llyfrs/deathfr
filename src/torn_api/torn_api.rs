use crate::database::structures::{ReviveEntry, TargetLastAction};
use log::{error};
use reqwest;
use serde_json;
use serde_json::Value;
use std::cmp::max;
use std::error::Error;
use std::thread;
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
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    ///
    /// Makes a request to the Torn API, using given url. It handles errors by waiting in case of rate limit exceeded, it alerts about invalid keys and panics in all other cases.
    ///
    pub async fn make_request(&mut self, url: &str) -> Result<Value, Box<dyn Error>> {
        loop {
            let url = format!("{}&comment={}", url, self.name);

            let result = reqwest::get(url).await?.text().await?;
            let json: Value = serde_json::from_str(&result)?;

            if json["error"].is_object() {
                if json["error"]["code"].as_u64().unwrap() == 5 {
                    sleep(Duration::from_secs(60)).await;
                    continue;
                }

                if json["error"]["code"].as_u64().unwrap() == 2 {
                    println!(
                        "error invalid key of owner: {}",
                        self.keys[(self.key_used - 1) % self.keys.len()].owner
                    );
                    error!(
                        "error invalid key of owner: {}",
                        self.keys[(self.key_used - 1) % self.keys.len()].owner
                    );

                    self.keys_limits.remove(self.key_used - 1);
                }
            }

            if self.keys_limits.len() == 0 {
                panic!("No valid keys left");
            }

            return Ok(json);
        }
    }

    /**
    Gets the next key to use,
    accessing keys using this function makes sure their limits are not exceeded,
    and when they run out of calls, it waits for the reset.
     */
    fn get_key(&mut self) -> Result<APIKey, Box<dyn Error>> {
        let mut key_to_use = self.key_used % self.keys.len();
        let start_key = key_to_use % self.keys.len();

        // If the key to be used is already out of calls we move to the next one,until we find one that can be used.
        while self.keys_limits[key_to_use].rate_limit <= 0 {
            self.key_used += 1;
            key_to_use = self.key_used % self.keys.len();

            // If we have checked all keys and all of them are out of calls, we wait for the reset.
            if key_to_use == start_key {
                thread::sleep(Duration::from_secs(max(
                    60 - (chrono::Utc::now().timestamp() - self.last_reset),
                    0,
                ) as u64));

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

    pub async fn get_player_data(&mut self, player_id: u64) -> Result<Value, Box<dyn Error>> {
        let key = self.get_key()?;

        let url = format!(
            "https://api.torn.com/user/{}?selections=profile&key={}",
            player_id, key.key
        );

        self.make_request(&url).await
    }

    pub async fn get_faction_data(&mut self, faction_id: u64) -> Result<Value, Box<dyn Error>> {
        let key = self.get_key()?;

        let url = format!(
            "https://api.torn.com/faction/{}?selections=basic&key={}",
            faction_id, key.key
        );

        self.make_request(&url).await
    }

    pub async fn get_revives(&mut self, from: u64) -> Option<(Vec<ReviveEntry>, u64)> {
        let mut revives = Vec::new();

        let key = self.get_key().ok()?;

        let url = format!(
            "https://api.torn.com/faction/?selections=revivesfull,basic&from={}&key={}",
            from, key.key
        );

        let json = self.make_request(&url).await.ok()?;
        let revives_json = json["revives"].as_object()?;

        let faction_id = json["ID"].as_u64().unwrap();

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

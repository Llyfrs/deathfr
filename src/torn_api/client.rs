use std::sync::Mutex;

use log::warn;
use tokio::time::{sleep, Duration};
use torn_api::executor::{Executor, ExecutorExt};
use torn_api::request::{ApiResponse, IntoRequest};
use torn_api::{ApiError, Error as TornError};
use torn_api::models::{FactionBasicResponse, FactionId, RevivesFullResponse, UserBasicResponse, UserDiscordPathId, UserProfileResponse};
use torn_api::parameters::ApiSortDesc;

/// A single Torn API key in the rotation pool.
#[derive(Clone)]
pub struct APIKey {
    pub key: String,
    pub rate_limit: u32,
    pub owner: String,
}

struct KeyEntry {
    /// Pre-configured reqwest client with the `Authorization: ApiKey {key}` header.
    client: reqwest::Client,
    owner: String,
    rate_limit: u32,
    remaining: u32,
}

struct RotationState {
    keys: Vec<KeyEntry>,
    last_reset: i64,
    key_used: usize,
}

/// Multi-key, rate-limited Torn API client.
///
/// Implements [`torn_api::executor::Executor`] for `&TornAPI` by selecting a key
/// from the rotation per request, replicating the previous hand-rolled wrapper's
/// behaviour: per-minute rate counting, key removal on invalid-key errors, and
/// backoff for transient errors.
#[derive(Clone)]
pub struct TornAPI {
    state: std::sync::Arc<Mutex<RotationState>>,
}

enum ErrorAction {
    RemoveKey,
    Retry(u64),
    Fatal,
}

fn classify_error_code(code: u16) -> ErrorAction {
    match code {
        // Key-specific errors — remove and retry with another key.
        1 | 2 | 10 | 13 | 16 | 18 => ErrorAction::RemoveKey,
        // Transient errors — wait and retry.
        5 => ErrorAction::Retry(60),
        8 | 9 => ErrorAction::Retry(30),
        15 | 17 => ErrorAction::Retry(5),
        // Request-level or unknown errors — do not retry or remove keys.
        _ => ErrorAction::Fatal,
    }
}

fn build_reqwest_client(api_key: &str) -> reqwest::Client {
    let mut headers = reqwest::header::HeaderMap::with_capacity(1);

    match reqwest::header::HeaderValue::from_str(&format!("ApiKey {api_key}")) {
        Ok(value) => {
            headers.insert(reqwest::header::AUTHORIZATION, value);
        }
        Err(e) => {
            warn!("Invalid API key header value (will omit Authorization header): {e}");
        }
    }

    reqwest::Client::builder()
        .default_headers(headers)
        .brotli(true)
        .build()
        .unwrap_or_else(|e| {
            warn!("Failed to build reqwest client: {e}");
            reqwest::Client::new()
        })
}

/// Result of trying to reserve a key for one request.
enum Pick {
    /// A key was reserved for use.
    Ready { client: reqwest::Client, owner: String },
    /// All keys are exhausted until the rate-limit window resets; sleep this many seconds.
    Exhausted { wait_secs: u64 },
    /// No keys remain in the pool at all.
    NoKeys,
}

fn pick_key(state: &mut RotationState) -> Pick {
    let now = chrono::Utc::now().timestamp();
    if now - state.last_reset >= 60 {
        state.last_reset = now;
        state.key_used = 0;
        for k in state.keys.iter_mut() {
            k.remaining = k.rate_limit;
        }
    }

    if state.keys.is_empty() {
        return Pick::NoKeys;
    }

    let n = state.keys.len();
    let start = state.key_used % n;
    let mut idx = start;
    loop {
        if state.keys[idx].remaining > 0 {
            state.keys[idx].remaining -= 1;
            state.key_used = idx + 1;
            let entry = &state.keys[idx];
            return Pick::Ready {
                client: entry.client.clone(),
                owner: entry.owner.clone(),
            };
        }
        idx = (idx + 1) % n;
        if idx == start {
            break;
        }
    }

    let wait = (60 - (now - state.last_reset)).max(1) as u64;
    Pick::Exhausted { wait_secs: wait }
}

impl TornAPI {
    /// Build a new client rotating over the provided keys.
    pub fn new(keys: Vec<APIKey>) -> TornAPI {
        let entries = keys
            .into_iter()
            .map(|k| {
                let client = build_reqwest_client(&k.key);
                KeyEntry {
                    client,
                    owner: k.owner,
                    rate_limit: k.rate_limit,
                    remaining: k.rate_limit,
                }
            })
            .collect();

        TornAPI {
            state: std::sync::Arc::new(Mutex::new(RotationState {
                keys: entries,
                last_reset: chrono::Utc::now().timestamp(),
                key_used: 0,
            })),
        }
    }

    /// Add a new API key to the rotation at runtime.
    pub async fn add_key(&self, key: APIKey) {
        let client = build_reqwest_client(&key.key);
        let entry = KeyEntry {
            client,
            owner: key.owner,
            rate_limit: key.rate_limit,
            remaining: key.rate_limit,
        };
        let mut state = self.state.lock().expect("rotation state mutex poisoned");
        state.keys.push(entry);
    }

    /// Fetch the full profile for a Torn user (by Torn user id *or* Discord id).
    pub async fn get_player_profile(
        &self,
        id: UserDiscordPathId,
    ) -> Result<UserProfileResponse, TornError> {
        self.user().profile_for_id(id, |b| b).await
    }

    /// Fetch the lightweight basic info for a Torn user (name + minimal fields).
    pub async fn get_player_basic(
        &self,
        id: UserDiscordPathId,
    ) -> Result<UserBasicResponse, TornError> {
        self.user().basic_for_id(id, |b| b).await
    }

    /// Fetch a faction's basic details (id, name, …).
    pub async fn get_faction_basic(
        &self,
        id: FactionId,
    ) -> Result<FactionBasicResponse, TornError> {
        self.faction().basic_for_id(id, |b| b).await
    }

    /// Fetch the key owner faction's simplified revives since `from`.
    pub async fn get_revives_full(&self, from: u64) -> Result<RevivesFullResponse, TornError> {
        self.faction()
            .revives_full(|b| b.api_from(from as i32).api_sort_desc(ApiSortDesc::Asc))
            .await
    }
}

impl Executor for &TornAPI {
    type Error = TornError;

    async fn execute<R>(self, request: R) -> (R::Discriminant, Result<ApiResponse, Self::Error>)
    where
        R: IntoRequest,
    {
        let (discriminant, api_request) = request.into_request();
        let url = api_request.url();

        loop {
            // --- reserve a key (brief critical section, no await held) ---
            let (client, owner) = match {
                let mut state = self.state.lock().expect("rotation state mutex poisoned");
                pick_key(&mut state)
            } {
                Pick::Ready { client, owner } => (client, owner),
                Pick::Exhausted { wait_secs } => {
                    sleep(Duration::from_secs(wait_secs)).await;
                    continue;
                }
                Pick::NoKeys => {
                    return (discriminant, Err(TornError::Api(ApiError::KeyIsEmpty)));
                }
            };

            // --- issue the HTTP request ---
            let response = match client.get(&url).send().await {
                Ok(r) => r,
                Err(e) => return (discriminant, Err(TornError::Network(e))),
            };
            let status = response.status();
            let bytes = match response.bytes().await {
                Ok(b) => b,
                Err(e) => return (discriminant, Err(TornError::Network(e))),
            };

            // --- classify Torn API errors and retry/remove/fatal accordingly ---
            if bytes.starts_with(br#"{"error":{"#) {
                #[derive(serde::Deserialize)]
                struct ErrorBody<'a> {
                    code: u16,
                    error: &'a str,
                }
                #[derive(serde::Deserialize)]
                struct ErrorContainer<'a> {
                    #[serde(borrow)]
                    error: ErrorBody<'a>,
                }

                if let Ok(container) = serde_json::from_slice::<ErrorContainer>(&bytes) {
                    let code = container.error.code;
                    let message = container.error.error;
                    match classify_error_code(code) {
                        ErrorAction::RemoveKey => {
                            {
                                let mut state =
                                    self.state.lock().expect("rotation state mutex poisoned");
                                state.keys.retain(|k| k.owner != owner);
                                state.key_used = 0;
                            }
                            warn!(
                                "Removed invalid Torn API key (owner {owner}, code {code}: {message})"
                            );
                            continue;
                        }
                        ErrorAction::Retry(secs) => {
                            warn!(
                                "Torn API error code {code} ({message}), retrying in {secs}s"
                            );
                            sleep(Duration::from_secs(secs)).await;
                            continue;
                        }
                        ErrorAction::Fatal => {
                            return (
                                discriminant,
                                Err(TornError::Api(ApiError::new(code, message))),
                            );
                        }
                    }
                }
            }

            return (discriminant, Ok(ApiResponse { status, body: Some(bytes) }));
        }
    }
}
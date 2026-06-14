use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::Arc;

use anyhow::Context as _;
use serde::Deserialize;
use serenity::all::{CommandInteraction, Message, MessageId, UserId};
use tokio::sync::Mutex;

use crate::bot::commands::contract::ListMessageInfo;
use crate::torn_api::{ReviveMonitor, ReviveSourceConfig, TornAPI};

/// Everything read from the secrets TOML file at startup.
pub struct LoadedSecrets {
    pub discord_token: String,
    pub database_url: String,
    pub secrets: Secrets,
}

//**
//  Holds all the required secrets for the bot to work
// *
#[derive(Debug, Clone)]
pub struct Secrets {
    pub revive_channel: u64,
    pub revive_role: u64,
    pub revive_faction_guilds: Vec<u64>,
    pub revive_faction: u64,
    pub owner_id: u64,
    pub admins: Vec<u64>,
    pub revive_faction_api_key: String,
    pub revive_sources: Vec<ReviveSourceConfig>,
    pub test_api_key: String,
    pub dev: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct SecretsConfig {
    discord_token: String,
    database_url: String,

    revive_channel: String,
    revive_role: String,
    revive_faction_guild_ids: Vec<String>,
    owner_id: String,
    admins: Vec<String>,

    revive_faction: Option<String>,
    #[serde(default)]
    revive_faction_api_key: Option<String>,

    #[serde(default)]
    revive_factions: HashMap<String, String>,

    test_api_key: String,

    // Keep this as string so existing Secrets.*.toml values like `DEV = "true"` still work.
    dev: String,
}

impl Secrets {
    pub fn load() -> anyhow::Result<LoadedSecrets> {
        let secrets_path = select_secrets_path()?;
        let secrets_raw = fs::read_to_string(&secrets_path)
            .with_context(|| format!("Failed to read secrets file '{secrets_path}'"))?;
        let cfg: SecretsConfig = toml::from_str(&secrets_raw)
            .with_context(|| format!("Failed to parse TOML secrets file '{secrets_path}'"))?;

        let revive_sources = parse_revive_factions(&cfg)?;

        let (revive_faction, revive_faction_api_key) = if !revive_sources.is_empty() {
            let first = revive_sources.first().unwrap();
            (
                *first.faction_ids.first().unwrap(),
                first.api_key.clone(),
            )
        } else {
            let faction = cfg
                .revive_faction
                .as_deref()
                .context("REVIVE_FACTION is required when REVIVE_FACTIONS is empty")?;
            let api_key = cfg
                .revive_faction_api_key
                .as_deref()
                .filter(|k| !k.is_empty())
                .context("REVIVE_FACTION_API_KEY is required when REVIVE_FACTIONS is empty")?;
            (parse_u64("REVIVE_FACTION", faction)?, api_key.to_string())
        };

        let secrets = Secrets {
            revive_channel: parse_u64("REVIVE_CHANNEL", &cfg.revive_channel)?,
            revive_role: parse_u64("REVIVE_ROLE", &cfg.revive_role)?,
            revive_faction_guilds: cfg
                .revive_faction_guild_ids
                .iter()
                .map(|x| parse_u64("REVIVE_FACTION_GUILD_ID[]", x))
                .collect::<anyhow::Result<Vec<u64>>>()?,
            revive_faction,
            owner_id: parse_u64("OWNER_ID", &cfg.owner_id)?,
            revive_faction_api_key,
            revive_sources,
            test_api_key: cfg.test_api_key,
            dev: parse_bool("DEV", &cfg.dev)?,
            admins: cfg
                .admins
                .iter()
                .map(|x| parse_u64("ADMINS[]", x))
                .collect::<anyhow::Result<Vec<u64>>>()?,
        };

        Ok(LoadedSecrets {
            discord_token: cfg.discord_token,
            database_url: cfg.database_url,
            secrets,
        })
    }

    pub fn is_revive_faction_guild(&self, guild_id: u64) -> bool {
        self.revive_faction_guilds.contains(&guild_id)
    }

    pub fn reviving_faction_ids(&self) -> Vec<u64> {
        if self.revive_sources.is_empty() {
            vec![self.revive_faction]
        } else {
            self.revive_sources
                .iter()
                .flat_map(|source| source.faction_ids.iter().copied())
                .collect()
        }
    }
}

fn parse_revive_factions(cfg: &SecretsConfig) -> anyhow::Result<Vec<ReviveSourceConfig>> {
    if cfg.revive_factions.is_empty() {
        return Ok(Vec::new());
    }

    let mut sources = Vec::with_capacity(cfg.revive_factions.len());

    for (id_str, api_key) in &cfg.revive_factions {
        let field = format!("REVIVE_FACTIONS.{id_str}");
        if api_key.is_empty() {
            anyhow::bail!("Invalid {field}: api key must be non-empty");
        }
        let faction_id = id_str
            .trim()
            .parse::<u64>()
            .with_context(|| format!("Invalid u64 for {field}: '{id_str}'"))?;
        sources.push(ReviveSourceConfig {
            api_key: api_key.clone(),
            faction_ids: vec![faction_id],
        });
    }

    Ok(sources)
}

fn select_secrets_path() -> anyhow::Result<String> {
    let args: Vec<String> = env::args().skip(1).collect();

    // `--secrets <path>` overrides everything
    if let Some(i) = args.iter().position(|a| a == "--secrets") {
        let path = args
            .get(i + 1)
            .context("Expected a path after '--secrets'")?;
        return Ok(path.clone());
    }

    // Explicit dev/prod flags
    if args.iter().any(|a| a == "--dev") {
        return Ok("Secrets.dev.toml".to_string());
    }
    if args.iter().any(|a| a == "--prod") {
        return Ok("Secrets.toml".to_string());
    }

    // Fallbacks: prefer prod if present, otherwise dev
    if std::path::Path::new("Secrets.toml").exists() {
        Ok("Secrets.toml".to_string())
    } else if std::path::Path::new("Secrets.dev.toml").exists() {
        Ok("Secrets.dev.toml".to_string())
    } else {
        anyhow::bail!(
            "No secrets file found. Create 'Secrets.toml' or 'Secrets.dev.toml' (or pass '--secrets <path>')."
        );
    }
}

fn parse_u64(field: &'static str, s: &str) -> anyhow::Result<u64> {
    s.trim()
        .parse::<u64>()
        .with_context(|| format!("Invalid u64 for {field}: '{s}'"))
}

fn parse_bool(field: &'static str, s: &str) -> anyhow::Result<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" => Ok(true),
        "false" | "0" | "no" | "n" => Ok(false),
        other => anyhow::bail!("Invalid bool for {field}: '{other}'"),
    }
}

/// Shared state passed to every poise command and event handler via the framework context.
pub struct Data {
    pub secrets: Secrets,
    pub torn_api: Arc<Mutex<TornAPI>>,
    pub revive_monitor: Arc<ReviveMonitor>,
    /// Map of messages sent to the reviver channel, keyed by the user that asked for a revive
    pub revive_responses: Mutex<HashMap<UserId, Message>>,
    /// Map of the original /reviveme interactions, keyed by the reviver-channel message id
    pub revive_cancellations: Mutex<HashMap<MessageId, CommandInteraction>>,
    /// Pagination state for /contract list messages
    pub contract_pages: Mutex<HashMap<MessageId, ListMessageInfo>>,
}

impl Data {
    pub fn new(secrets: Secrets, torn_api: TornAPI, revive_monitor: Arc<ReviveMonitor>) -> Self {
        Self {
            secrets,
            torn_api: Arc::new(Mutex::new(torn_api)),
            revive_monitor,
            revive_responses: Mutex::new(HashMap::new()),
            revive_cancellations: Mutex::new(HashMap::new()),
            contract_pages: Mutex::new(HashMap::new()),
        }
    }
}

pub type Error = anyhow::Error;
pub type Context<'a> = poise::Context<'a, Data, Error>;

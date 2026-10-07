//! Live contract channel ("live board").
//!
//! Every guild can configure one channel that the bot keeps filled with one message per
//! active / upcoming contract (plus a header), with live statistics for active contracts.
//! The channel is owned by the bot: anything else posted into it is deleted.
//!
//! Message layout is persisted in the database, so restarts edit the existing messages
//! instead of reposting them. Discord orders messages by creation time, so reordering is
//! done by reposting the smallest suffix of messages that is out of place.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use mongodb::bson::{doc, Bson};
use serenity::all::{
    ChannelId, Colour, CreateEmbed, CreateMessage, EditMessage, GetMessages, GuildId, Http,
    HttpError, MessageId,
};
use tokio::sync::{Mutex, Notify, RwLock};
use torn_api::models::FactionId;

use crate::bot::tools::get_player_cache::get_player_cache;
use crate::bot::tools::promote_pending::promote_pending_contracts;
use crate::database::structures::{Contract, LiveChannel, LiveMessage, ReviveEntry, Status};
use crate::database::Database;
use crate::pricing::{classify_revive, format_with_commas, ReviveClass, ReviveCounts};
use crate::torn_api::TornAPI;

const HEADER_KEY: &str = "header";
/// Longest time between two refreshes when nothing asks for one.
const MAX_IDLE: Duration = Duration::from_secs(15 * 60);
/// Coalesces bursts of refresh requests (e.g. several contracts ended in a row).
const DEBOUNCE: Duration = Duration::from_secs(3);
/// Upper bound of history pages (100 messages each) inspected per refresh.
const HISTORY_PAGES: usize = 10;
const TOP_REVIVERS: usize = 3;
/// Embed titles are limited to 256 characters; leaves room for the status emoji.
const TITLE_MAX: usize = 240;
/// Discord refuses to bulk delete messages older than 14 days.
const BULK_DELETE_MAX_AGE_SECS: i64 = 14 * 24 * 60 * 60 - 60 * 60;
const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;

// https://discord.com/developers/docs/topics/opcodes-and-status-codes#json-json-error-codes
const UNKNOWN_CHANNEL: isize = 10003;
const UNKNOWN_MESSAGE: isize = 10008;
const MISSING_ACCESS: isize = 50001;
const MISSING_PERMISSIONS: isize = 50013;

const COLOUR_HEADER: Colour = Colour(0x5865F2);
const COLOUR_ACTIVE: Colour = Colour(0x57F287);
const COLOUR_PENDING: Colour = Colour(0xFEE75C);

struct ChannelState {
    guild_id: GuildId,
    board_messages: HashSet<MessageId>,
}

impl ChannelState {
    fn of(config: &LiveChannel) -> Self {
        Self {
            guild_id: GuildId::new(config.guild_id),
            board_messages: config
                .messages
                .iter()
                .map(|m| MessageId::new(m.message_id))
                .collect(),
        }
    }
}

struct Card {
    key: String,
    embed: CreateEmbed,
    hash: String,
}

impl Card {
    fn new(key: impl Into<String>, embed: CreateEmbed) -> Self {
        let mut hasher = DefaultHasher::new();
        serde_json::to_string(&embed)
            .unwrap_or_default()
            .hash(&mut hasher);
        Self {
            key: key.into(),
            embed,
            hash: format!("{:016x}", hasher.finish()),
        }
    }
}

/// Rendered state of the board, shared by every configured channel.
struct Board {
    cards: Vec<Card>,
    /// Start time of the next pending contract, so the loop can wake up when it goes live.
    next_transition: Option<u64>,
}

/// What a channel refresh did, for the admin that triggered it.
#[derive(Default, Debug)]
pub struct RefreshReport {
    pub posted: usize,
    pub edited: usize,
    pub removed: usize,
    pub purged: usize,
    pub warnings: Vec<String>,
}

pub struct LiveBoard {
    torn_api: Arc<TornAPI>,
    reviving_faction_ids: Vec<u64>,
    notify: Arc<Notify>,
    /// Serializes refreshes and configuration changes.
    refresh_lock: Mutex<()>,
    /// Configured live channels and their board messages, so the message event handlers
    /// need no database lookup.
    channels: RwLock<HashMap<ChannelId, ChannelState>>,
    faction_names: Mutex<HashMap<u64, String>>,
}

impl LiveBoard {
    pub fn new(
        torn_api: Arc<TornAPI>,
        reviving_faction_ids: Vec<u64>,
        notify: Arc<Notify>,
    ) -> Self {
        Self {
            torn_api,
            reviving_faction_ids,
            notify,
            refresh_lock: Mutex::new(()),
            channels: RwLock::new(HashMap::new()),
            faction_names: Mutex::new(HashMap::new()),
        }
    }

    /// Asks the background loop to refresh every live channel soon. Cheap and non-blocking;
    /// multiple requests in a short window collapse into one refresh.
    pub fn request_refresh(&self) {
        self.notify.notify_one();
    }

    pub async fn is_live_channel(&self, channel_id: ChannelId) -> bool {
        self.channels.read().await.contains_key(&channel_id)
    }

    async fn load_channels(&self) -> anyhow::Result<()> {
        let configs = Database::get_collection::<LiveChannel>().await?;
        let mut channels = self.channels.write().await;
        channels.clear();
        for config in configs {
            channels.insert(ChannelId::new(config.channel_id), ChannelState::of(&config));
        }
        Ok(())
    }

    pub async fn run_loop(self: Arc<Self>, http: Arc<Http>) {
        log::info!("Starting live board loop");

        if let Err(e) = self.load_channels().await {
            log::error!("Failed to load live channels: {e:#}");
        }

        loop {
            let next_transition = match self.refresh_all(&http).await {
                Ok(next) => next,
                Err(e) => {
                    log::error!("Live board refresh failed: {e:#}");
                    None
                }
            };

            let now = Utc::now().timestamp() as u64;
            let wait = next_transition
                // A few seconds of slack so the contract is definitely past its start time.
                .map(|start| Duration::from_secs(start.saturating_sub(now) + 5))
                .unwrap_or(MAX_IDLE)
                .min(MAX_IDLE);

            tokio::select! {
                _ = self.notify.notified() => {}
                _ = tokio::time::sleep(wait) => {}
            }
            tokio::time::sleep(DEBOUNCE).await;
        }
    }

    /// Refreshes every configured live channel.
    async fn refresh_all(&self, http: &Arc<Http>) -> anyhow::Result<Option<u64>> {
        let _guard = self.refresh_lock.lock().await;

        let configs = Database::get_collection::<LiveChannel>().await?;
        if configs.is_empty() {
            // Nothing to render, but keep pending contracts flipping on time.
            promote_pending_contracts().await?;
            return Ok(None);
        }

        let board = self.render().await?;

        for config in configs {
            let guild_id = config.guild_id;
            match self.sync_channel(http, config, &board).await {
                Ok(report) => {
                    for warning in &report.warnings {
                        log::warn!("Live channel in guild {guild_id}: {warning}");
                    }
                }
                Err(e) => log::error!("Failed to refresh live channel in guild {guild_id}: {e:#}"),
            }
        }

        Ok(board.next_transition)
    }

    /// Refreshes the live channel of one guild right away.
    pub async fn refresh_guild(
        &self,
        http: &Arc<Http>,
        guild_id: GuildId,
    ) -> anyhow::Result<Option<RefreshReport>> {
        let _guard = self.refresh_lock.lock().await;

        let Some(config) = Self::config_for(guild_id).await? else {
            return Ok(None);
        };

        let board = self.render().await?;
        self.sync_channel(http, config, &board).await.map(Some)
    }

    /// Makes `channel_id` the live channel of `guild_id` and fills it.
    /// Messages already in the channel are left alone; only newer ones are purged.
    pub async fn set_channel(
        &self,
        http: &Arc<Http>,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) -> anyhow::Result<RefreshReport> {
        let _guard = self.refresh_lock.lock().await;

        let mut warnings = Vec::new();

        let config = match Self::config_for(guild_id).await? {
            Some(existing) if existing.channel_id == channel_id.get() => existing,
            previous => {
                if let Some(previous) = previous {
                    let removed = self.remove_board(http, &previous).await;
                    if let Err(e) = removed {
                        warnings.push(format!(
                            "Could not clean up the previous live channel <#{}>: {}",
                            previous.channel_id,
                            describe_error(&e)
                        ));
                    }
                }

                LiveChannel {
                    guild_id: guild_id.get(),
                    channel_id: channel_id.get(),
                    configured_after: snowflake_now(),
                    messages: Vec::new(),
                }
            }
        };

        self.save(&config).await?;

        let board = self.render().await?;
        let mut report = self.sync_channel(http, config, &board).await?;
        warnings.append(&mut report.warnings);
        report.warnings = warnings;
        Ok(report)
    }

    /// Stops maintaining the live channel of `guild_id` and deletes the board messages.
    /// Returns false when no live channel was configured.
    pub async fn disable(&self, http: &Arc<Http>, guild_id: GuildId) -> anyhow::Result<bool> {
        let _guard = self.refresh_lock.lock().await;

        let Some(config) = Self::config_for(guild_id).await? else {
            return Ok(false);
        };

        Database::delete_one::<LiveChannel>(doc! { "guild_id": guild_id.get() as i64 }).await?;
        self.channels
            .write()
            .await
            .retain(|_, state| state.guild_id != guild_id);

        if let Err(e) = self.remove_board(http, &config).await {
            log::warn!(
                "Failed to delete live board messages in channel {}: {e:#}",
                config.channel_id
            );
        }

        Ok(true)
    }

    async fn config_for(guild_id: GuildId) -> anyhow::Result<Option<LiveChannel>> {
        Ok(Database::get_collection_with_filter::<LiveChannel>(Some(
            doc! { "guild_id": guild_id.get() as i64 },
        ))
        .await?
        .pop())
    }

    async fn save(&self, config: &LiveChannel) -> anyhow::Result<()> {
        Database::replace_upsert(config, doc! { "guild_id": config.guild_id as i64 }).await?;

        let guild_id = GuildId::new(config.guild_id);
        let mut channels = self.channels.write().await;
        channels.retain(|_, state| state.guild_id != guild_id);
        channels.insert(ChannelId::new(config.channel_id), ChannelState::of(config));
        Ok(())
    }

    async fn remove_board(&self, http: &Arc<Http>, config: &LiveChannel) -> serenity::Result<()> {
        let channel = ChannelId::new(config.channel_id);
        let ids: Vec<MessageId> = config
            .messages
            .iter()
            .map(|m| MessageId::new(m.message_id))
            .collect();
        delete_individually(http, channel, &ids).await.map(|_| ())
    }

    /// Brings one channel in line with `board`, persisting the resulting layout even when
    /// the sync fails halfway through.
    async fn sync_channel(
        &self,
        http: &Arc<Http>,
        mut config: LiveChannel,
        board: &Board,
    ) -> anyhow::Result<RefreshReport> {
        let mut report = RefreshReport::default();
        let result = self
            .sync_channel_inner(http, &mut config, board, &mut report)
            .await;

        let channel_gone = matches!(
            &result,
            Err(e) if discord_code(e) == Some(UNKNOWN_CHANNEL)
        );

        if channel_gone {
            log::warn!(
                "Live channel {} of guild {} no longer exists, disabling it",
                config.channel_id,
                config.guild_id
            );
            Database::delete_one::<LiveChannel>(doc! { "guild_id": config.guild_id as i64 })
                .await?;
            self.channels
                .write()
                .await
                .remove(&ChannelId::new(config.channel_id));
            anyhow::bail!("The live channel was deleted, so it has been disabled.");
        }

        self.save(&config).await?;

        match result {
            Ok(()) => Ok(report),
            Err(e) => Err(anyhow::anyhow!(describe_error(&e))),
        }
    }

    async fn sync_channel_inner(
        &self,
        http: &Arc<Http>,
        config: &mut LiveChannel,
        board: &Board,
        report: &mut RefreshReport,
    ) -> serenity::Result<()> {
        let channel = ChannelId::new(config.channel_id);

        // 1. Look at what is actually in the channel: drop messages someone deleted from
        //    our layout and purge everything we did not post.
        match fetch_history(http, channel, config.configured_after).await {
            Ok(present) => {
                config
                    .messages
                    .retain(|m| present.contains(&MessageId::new(m.message_id)));

                let tracked: HashSet<MessageId> = config
                    .messages
                    .iter()
                    .map(|m| MessageId::new(m.message_id))
                    .collect();
                let strays: Vec<MessageId> = present.difference(&tracked).copied().collect();

                if !strays.is_empty() {
                    match purge(http, channel, &strays).await {
                        Ok(count) => report.purged += count,
                        Err(e) if is_permission_error(&e) => report.warnings.push(
                            "Missing **Manage Messages** permission, other messages in the channel cannot be removed. \
                             Consider making the channel read-only for everyone except the bot."
                                .to_string(),
                        ),
                        Err(e) => return Err(e),
                    }
                }
            }
            Err(e) if discord_code(&e) == Some(UNKNOWN_CHANNEL) => return Err(e),
            Err(e) => {
                log::warn!("Could not read history of live channel {channel}: {e}");
                report.warnings.push(
                    "Missing **Read Message History** permission, the channel cannot be checked for \
                     deleted or foreign messages."
                        .to_string(),
                );
            }
        }

        // 2. Remove contracts that are no longer on the board (ended / deleted).
        let wanted: HashSet<&str> = board.cards.iter().map(|c| c.key.as_str()).collect();
        let (keep, gone): (Vec<LiveMessage>, Vec<LiveMessage>) =
            std::mem::take(&mut config.messages)
                .into_iter()
                .partition(|m| wanted.contains(m.key.as_str()));
        config.messages = keep;

        let gone_ids: Vec<MessageId> = gone.iter().map(|m| MessageId::new(m.message_id)).collect();
        report.removed += delete_individually(http, channel, &gone_ids).await?;

        // 3. Messages that are already in the right position only need an edit.
        let mut in_place = config
            .messages
            .iter()
            .zip(&board.cards)
            .take_while(|(message, card)| message.key == card.key)
            .count();

        for i in 0..in_place {
            if config.messages[i].hash == board.cards[i].hash {
                continue;
            }

            let edit = EditMessage::new()
                .content("")
                .embed(board.cards[i].embed.clone());
            match channel
                .edit_message(http, MessageId::new(config.messages[i].message_id), edit)
                .await
            {
                Ok(_) => {
                    config.messages[i].hash = board.cards[i].hash.clone();
                    report.edited += 1;
                }
                // Deleted since we read the history; everything from here on gets reposted.
                Err(e) if discord_code(&e) == Some(UNKNOWN_MESSAGE) => {
                    in_place = i;
                    break;
                }
                Err(e) => return Err(e),
            }
        }

        // 4. Everything after the first misplaced message is reposted in order, since
        //    Discord cannot reorder messages.
        let misplaced: Vec<MessageId> = config.messages[in_place..]
            .iter()
            .map(|m| MessageId::new(m.message_id))
            .collect();
        config.messages.truncate(in_place);
        report.removed += delete_individually(http, channel, &misplaced).await?;

        for card in &board.cards[in_place..] {
            let message = channel
                .send_message(http, CreateMessage::new().embed(card.embed.clone()))
                .await?;
            config.messages.push(LiveMessage {
                key: card.key.clone(),
                message_id: message.id.get(),
                hash: card.hash.clone(),
            });
            report.posted += 1;
        }

        Ok(())
    }

    async fn render(&self) -> anyhow::Result<Board> {
        if let Err(e) = promote_pending_contracts().await {
            log::error!("Failed to promote pending contracts: {e:#}");
        }

        let mut contracts = Database::get_collection_with_filter::<Contract>(Some(doc! {
            "status": { "$in": [
                mongodb::bson::to_bson(&Status::Active)?,
                mongodb::bson::to_bson(&Status::Pending)?,
            ] }
        }))
        .await?;

        // Chronological: running contracts first, then upcoming ones by start time. Since
        // pending contracts always start after active ones, a pending contract going live
        // keeps its position and only needs an edit.
        contracts.sort_by(|a, b| {
            a.started
                .cmp(&b.started)
                .then_with(|| a.contract_id.cmp(&b.contract_id))
        });

        let last_sync = Database::get_value::<i64>("last_update").await;
        let now = Utc::now().timestamp() as u64;

        let active = contracts
            .iter()
            .filter(|c| c.status == Status::Active)
            .count();
        let pending = contracts.len() - active;

        let mut cards = vec![Card::new(
            HEADER_KEY,
            header_embed(active, pending, last_sync),
        )];

        for contract in &contracts {
            let embed = match contract.status {
                Status::Active => self.active_embed(contract).await?,
                _ => self.pending_embed(contract).await,
            };
            cards.push(Card::new(contract.contract_id.clone(), embed));
        }

        let next_transition = contracts
            .iter()
            .filter(|c| c.status == Status::Pending && c.started > now)
            .map(|c| c.started)
            .min();

        Ok(Board {
            cards,
            next_transition,
        })
    }

    async fn pending_embed(&self, contract: &Contract) -> CreateEmbed {
        let faction = self.faction_link(contract.faction_id).await;

        CreateEmbed::new()
            .title(format!(
                "🕒 {}",
                truncate(&contract.contract_name, TITLE_MAX)
            ))
            .colour(COLOUR_PENDING)
            .description(format!(
                "**Upcoming** · starts <t:{0}:R> (<t:{0}:f>)\nTarget: {faction}",
                contract.started
            ))
            .field("Contract ID", format!("`{}`", contract.contract_id), true)
            .field("Min Chance", format!("{}%", contract.min_chance), true)
            .field("Pricing", contract.pricing_type.label(), true)
    }

    async fn active_embed(&self, contract: &Contract) -> anyhow::Result<CreateEmbed> {
        let faction = self.faction_link(contract.faction_id).await;

        let reviver_factions: Vec<Bson> = self
            .reviving_faction_ids
            .iter()
            .map(|id| Bson::Int64(*id as i64))
            .collect();

        let revives = Database::get_collection_with_filter::<ReviveEntry>(Some(doc! {
            "timestamp": { "$gte": Bson::Int64(contract.started as i64) },
            "target_faction": Bson::Int64(contract.faction_id as i64),
            "reviver_faction": { "$in": reviver_factions }
        }))
        .await?;

        let mut successful = 0u64;
        let mut failed_counted = 0u64;
        let mut failed_ignored = 0u64;
        let mut chance_sum = 0f64;
        let mut last_revive = None;
        // reviver id -> (successful, failed counted)
        let mut per_reviver: HashMap<u64, (u64, u64)> = HashMap::new();

        for revive in &revives {
            chance_sum += revive.chance as f64;
            last_revive = last_revive.max(Some(revive.timestamp));
            let entry = per_reviver.entry(revive.reviver_id).or_default();
            match classify_revive(revive, contract.min_chance) {
                ReviveClass::Success => {
                    successful += 1;
                    entry.0 += 1;
                }
                ReviveClass::FailedCounted => {
                    failed_counted += 1;
                    entry.1 += 1;
                }
                ReviveClass::Ignored => failed_ignored += 1,
            }
        }

        let total = revives.len() as u64;
        let price = contract.pricing_type.calculate(
            ReviveCounts {
                successful,
                failed_counted,
            },
            contract.faction_cut,
        );

        let success_rate = if total > 0 {
            format!("{:.1}%", successful as f64 / total as f64 * 100.0)
        } else {
            "—".to_string()
        };
        let avg_chance = if total > 0 {
            format!("{:.1}%", chance_sum / total as f64)
        } else {
            "—".to_string()
        };
        let last_revive = last_revive
            .map(|t| format!("<t:{t}:R>"))
            .unwrap_or_else(|| "—".to_string());

        let mut embed = CreateEmbed::new()
            .title(format!(
                "🟢 {}",
                truncate(&contract.contract_name, TITLE_MAX)
            ))
            .colour(COLOUR_ACTIVE)
            .description(format!(
                "**Active** · started <t:{0}:R> (<t:{0}:f>)\nTarget: {faction}",
                contract.started
            ))
            .field("Contract ID", format!("`{}`", contract.contract_id), true)
            .field("Min Chance", format!("{}%", contract.min_chance), true)
            .field("Pricing", contract.pricing_type.label(), true)
            .field("Successful", successful.to_string(), true)
            .field("Failed (counted)", failed_counted.to_string(), true)
            .field("Failed (ignored)", failed_ignored.to_string(), true)
            .field("Success Rate", success_rate, true)
            .field("Avg Chance", avg_chance, true)
            .field("Last Revive", last_revive, true)
            .field(
                "Running Total",
                format!("${}", format_with_commas(price.final_with_markup)),
                true,
            )
            .field("Revivers", per_reviver.len().to_string(), true);

        if !per_reviver.is_empty() {
            let mut ranking: Vec<(u64, (u64, u64))> = per_reviver.into_iter().collect();
            ranking.sort_by(|a, b| {
                let score = |(s, f): (u64, u64)| s + f;
                score(b.1)
                    .cmp(&score(a.1))
                    .then(b.1 .0.cmp(&a.1 .0))
                    .then(a.0.cmp(&b.0))
            });

            let mut lines = Vec::new();
            for (place, (reviver_id, (success, failed))) in
                ranking.into_iter().take(TOP_REVIVERS).enumerate()
            {
                let name = get_player_cache(reviver_id, &self.torn_api)
                    .await
                    .map(|p| escape_markdown(&p.name))
                    .unwrap_or_else(|| reviver_id.to_string());
                lines.push(format!(
                    "{}. **{name}** — {success} successful, {failed} failed",
                    place + 1
                ));
            }
            embed = embed.field("Top Revivers", lines.join("\n"), false);
        }

        Ok(embed)
    }

    /// "[Name](profile link) [ID]", with names cached for the lifetime of the process.
    async fn faction_link(&self, faction_id: u64) -> String {
        let cached = self.faction_names.lock().await.get(&faction_id).cloned();

        let name = match cached {
            Some(name) => Some(name),
            None => match self
                .torn_api
                .get_faction_basic(FactionId::new(faction_id as i32))
                .await
            {
                Ok(data) => {
                    let name = data.basic.name;
                    self.faction_names
                        .lock()
                        .await
                        .insert(faction_id, name.clone());
                    Some(name)
                }
                Err(e) => {
                    log::warn!("Failed to fetch faction {faction_id} for live board: {e:#}");
                    None
                }
            },
        };

        let name = name.unwrap_or_else(|| "Faction".to_string());
        format!(
            "[{}](https://www.torn.com/factions.php?step=profile&ID={faction_id}) [{faction_id}]",
            escape_markdown(&name)
        )
    }
}

fn header_embed(active: usize, pending: usize, last_sync: Option<i64>) -> CreateEmbed {
    let mut description = if active + pending == 0 {
        "There are no active or upcoming contracts right now.".to_string()
    } else {
        format!("**{active}** active · **{pending}** upcoming")
    };

    description.push_str("\n\n");
    match last_sync {
        Some(t) => description.push_str(&format!(
            "Revive data last synced <t:{t}:R>. Statistics update after every sync (about hourly)."
        )),
        None => description.push_str("Revive data has not been synced yet."),
    }

    CreateEmbed::new()
        .title("📡 Live Contracts")
        .colour(COLOUR_HEADER)
        .description(description)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut truncated: String = text.chars().take(max - 1).collect();
        truncated.push('…');
        truncated
    }
}

fn escape_markdown(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '*' | '_' | '~' | '`' | '|' | '[' | ']' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// Snowflake corresponding to the current time, usable as a message ID cursor.
fn snowflake_now() -> u64 {
    let now_ms = Utc::now().timestamp_millis() as u64;
    now_ms.saturating_sub(DISCORD_EPOCH_MS) << 22
}

/// IDs of all messages posted after `after` (bounded by [`HISTORY_PAGES`]).
async fn fetch_history(
    http: &Arc<Http>,
    channel: ChannelId,
    after: u64,
) -> serenity::Result<HashSet<MessageId>> {
    let mut ids = HashSet::new();
    let mut cursor = MessageId::new(after.max(1));

    for _ in 0..HISTORY_PAGES {
        let page = channel
            .messages(http, GetMessages::new().after(cursor).limit(100))
            .await?;
        let len = page.len();

        for message in page {
            cursor = cursor.max(message.id);
            ids.insert(message.id);
        }

        if len < 100 {
            break;
        }
    }

    Ok(ids)
}

/// Deletes messages one by one, ignoring ones that are already gone.
/// Works on the bot's own messages without the Manage Messages permission.
async fn delete_individually(
    http: &Arc<Http>,
    channel: ChannelId,
    ids: &[MessageId],
) -> serenity::Result<usize> {
    let mut deleted = 0;
    for id in ids {
        match channel.delete_message(http, *id).await {
            Ok(()) => deleted += 1,
            Err(e) if discord_code(&e) == Some(UNKNOWN_MESSAGE) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(deleted)
}

/// Deletes arbitrary messages, bulk deleting where Discord allows it.
async fn purge(http: &Arc<Http>, channel: ChannelId, ids: &[MessageId]) -> serenity::Result<usize> {
    let cutoff = Utc::now().timestamp() - BULK_DELETE_MAX_AGE_SECS;
    let (recent, old): (Vec<MessageId>, Vec<MessageId>) = ids
        .iter()
        .partition(|id| id.created_at().unix_timestamp() > cutoff);

    let mut deleted = 0;
    for chunk in recent.chunks(100) {
        if chunk.len() < 2 {
            deleted += delete_individually(http, channel, chunk).await?;
            continue;
        }
        match channel.delete_messages(http, chunk).await {
            Ok(()) => deleted += chunk.len(),
            Err(e) if is_permission_error(&e) => return Err(e),
            // E.g. one of them was deleted meanwhile; fall back to one by one.
            Err(_) => deleted += delete_individually(http, channel, chunk).await?,
        }
    }

    deleted += delete_individually(http, channel, &old).await?;
    Ok(deleted)
}

fn discord_code(error: &serenity::Error) -> Option<isize> {
    match error {
        serenity::Error::Http(HttpError::UnsuccessfulRequest(response)) => {
            Some(response.error.code)
        }
        _ => None,
    }
}

fn is_permission_error(error: &serenity::Error) -> bool {
    matches!(
        discord_code(error),
        Some(MISSING_ACCESS | MISSING_PERMISSIONS)
    )
}

/// Human-readable explanation of a Discord error, with a hint for permission problems.
fn describe_error(error: &serenity::Error) -> String {
    if is_permission_error(error) {
        format!(
            "the bot is missing permissions in the live channel ({error}). It needs View Channel, \
             Send Messages, Embed Links, Read Message History and Manage Messages."
        )
    } else {
        error.to_string()
    }
}

/// Deletes a message posted into a live channel by someone other than the bot.
/// The bot's own untracked messages (e.g. command replies) are cleaned up by the next refresh,
/// since board messages are only tracked after they have been posted.
pub async fn handle_message(
    ctx: &serenity::all::Context,
    board: &LiveBoard,
    message: &serenity::all::Message,
) {
    if message.author.id == ctx.cache.current_user().id {
        return;
    }
    if !board.is_live_channel(message.channel_id).await {
        return;
    }

    match message.delete(&ctx.http).await {
        Ok(()) => {}
        Err(e) if discord_code(&e) == Some(UNKNOWN_MESSAGE) => {}
        Err(e) if is_permission_error(&e) => log::debug!(
            "Cannot delete message in live channel {} (missing permissions)",
            message.channel_id
        ),
        Err(e) => log::warn!(
            "Failed to delete message in live channel {}: {e}",
            message.channel_id
        ),
    }
}

/// Refreshes the board soon when someone deleted one of its messages.
pub async fn handle_messages_deleted(board: &LiveBoard, channel_id: ChannelId, ids: &[MessageId]) {
    let affects_board = board
        .channels
        .read()
        .await
        .get(&channel_id)
        .is_some_and(|state| ids.iter().any(|id| state.board_messages.contains(id)));

    if affects_board {
        board.request_refresh();
    }
}

impl RefreshReport {
    pub fn summary(&self) -> String {
        let mut summary = format!(
            "Posted {}, edited {}, removed {} board message(s); purged {} other message(s).",
            self.posted, self.edited, self.removed, self.purged
        );
        for warning in &self.warnings {
            summary.push_str("\n⚠️ ");
            summary.push_str(warning);
        }
        summary
    }
}

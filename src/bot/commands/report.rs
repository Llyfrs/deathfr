use crate::bot::auth::{level_of, AccessLevel};
use crate::bot::data::{Context, Error};
use crate::bot::tools::get_player_cache::get_player_cache;
use crate::database::structures::{Contract, ReviveEntry, Status};
use crate::database::Database;
use mongodb::bson::{doc, Bson};
use poise::CreateReply;
use serenity::builder::{CreateEmbed, CreateMessage};
use std::collections::HashMap;

/// Generate contract report
#[poise::command(slash_command)]
pub async fn report(
    ctx: Context<'_>,
    #[description = "The contract ID of the player you want to report"] contract_id: String,
) -> Result<(), Error> {
    let secrets = &ctx.data().secrets;

    let is_admin = level_of(&ctx) >= AccessLevel::Admin;

    let contract = Database::get_collection_with_filter::<Contract>(Some(doc! {
        "contract_id": contract_id.clone()
    }))
    .await
    .unwrap()
    .pop();

    let Some(mut contract) = contract else {
        ctx.send(
            CreateReply::default()
                .content("Contract not found")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    };

    if contract.status != Status::Ended {
        let msg = match contract.status {
            Status::Pending => format!(
                "Contract hasn't started yet — it starts <t:{}:f>.",
                contract.started
            ),
            _ => "Contract is still active. Live reports will be implemented in the future hopefully."
                .to_string(),
        };
        ctx.send(CreateReply::default().content(msg).ephemeral(true))
            .await?;
        return Ok(());
    }

    let syncing_status = if !contract.revives_synced {
        ctx.defer().await?;
        let status = ctx
            .send(
                CreateReply::default().content("Generating report — syncing revive data…"),
            )
            .await?;

        if let Err(e) = ctx
            .data()
            .revive_monitor
            .sync_for_contract(contract.ended)
            .await
        {
            status
                .edit(
                    ctx,
                    CreateReply::default()
                        .content(format!("Failed to sync revive data: {e:#}")),
                )
                .await?;
            return Ok(());
        }

        contract.revives_synced = true;
        Database::update(contract.clone(), doc! {"contract_id": contract_id.clone()})
            .await
            .unwrap();

        Some(status)
    } else {
        None
    };

    let reviving_faction_ids = secrets.reviving_faction_ids();
    let reviver_faction_filter: Vec<Bson> = reviving_faction_ids
        .iter()
        .map(|id| Bson::Int64(*id as i64))
        .collect();

    let revives = Database::get_collection_with_filter::<ReviveEntry>(Some(doc! {
        "timestamp": {
            "$gte": Bson::Int64(contract.started as i64),
            "$lte": Bson::Int64(contract.ended as i64)
        },
        "target_faction": Bson::Int64(contract.faction_id as i64),
        "reviver_faction": { "$in": reviver_faction_filter }
    }))
    .await
    .unwrap();

    let mut per_faction_player: HashMap<(u64, u64), Vec<ReviveEntry>> = HashMap::new();
    let mut successful = 0;
    let mut failed = 0;
    let len = revives.len();

    for revive in revives {
        per_faction_player
            .entry((revive.reviver_faction, revive.reviver_id))
            .or_default()
            .push(revive.clone());

        if revive.result == "success" {
            successful += 1;
        } else if revive.chance > contract.min_chance as f32 {
            failed += 1;
        }
    }

    let api = ctx.data().torn_api.clone();

    let faction_data_target = api
        .lock()
        .await
        .get_faction_data(contract.faction_id)
        .await
        .unwrap();

    let mut faction_names: HashMap<u64, String> = HashMap::new();
    let mut reviver_faction_labels = Vec::new();
    for id in &reviving_faction_ids {
        let faction_data = api.lock().await.get_faction_data(*id).await.unwrap();
        let label = format!(
            "{} ({})",
            faction_data["name"].as_str().unwrap(),
            faction_data["ID"].as_u64().unwrap()
        );
        faction_names.insert(*id, label.clone());
        reviver_faction_labels.push(label);
    }

    let reviver_field_name = if reviving_faction_ids.len() == 1 {
        "Reviving Faction"
    } else {
        "Reviving Factions"
    };

    let price = vec![
        format_with_commas((successful * 900000 + failed * 1000000) as u64),
        format_with_commas(
            (successful * 900000 + failed * 1000000) as u64
                * (1.0 + contract.faction_cut as f64 / 100.0) as u64,
        ),
    ];

    let mut embed = CreateEmbed::new()
        .title(contract.contract_name.clone() + " Report")
        .description(" ")
        .field(
            reviver_field_name,
            reviver_faction_labels.join("\n"),
            true,
        )
        .field(
            "Target Faction",
            format!(
                "{} ({})",
                faction_data_target["name"].as_str().unwrap(),
                faction_data_target["ID"].as_u64().unwrap()
            ),
            true,
        )
        .field("", "", false)
        .field("Successful Revives", successful.to_string(), true)
        .field("Failed Counted", (failed).to_string(), true)
        .field(
            "Failed Ignored",
            (len - successful - failed).to_string(),
            true,
        )
        .field("Started", format!("<t:{}:f>", contract.started), true)
        .field("Ended", format!("<t:{}:f>", contract.ended), true)
        .field("", "", false)
        .field(
            "Final Price",
            price
                .get(!is_admin as usize)
                .unwrap_or(&"".to_string())
                .to_string(),
            true,
        );

    if is_admin {
        embed = embed.field(
            format!("Final Price (+{}%)", contract.faction_cut),
            format!(
                "${}",
                format_with_commas(
                    ((successful * 900000 + failed * 1000000) as f64
                        * (1.0 + contract.faction_cut as f64 / 100.0)) as u64
                )
            ),
            true,
        );
    };

    if let Some(status) = syncing_status {
        status.delete(ctx).await?;
    }

    ctx.send(CreateReply::default().embed(embed)).await?;

    // List of rewards is only for admins
    if !is_admin {
        return Ok(());
    }

    let mut per_faction_rewards: HashMap<u64, Vec<(u64, String)>> = HashMap::new();

    for ((faction_id, player_id), entries) in &per_faction_player {
        // TODO: caching system for revive skill to avoid rate limits on large reports
        let player_data = match get_player_cache(*player_id, &mut *api.lock().await).await {
            Some(player) => player,
            None => continue,
        };

        let success = entries
            .iter()
            .filter(|r| r.result == "success")
            .count();

        let failed_counted = entries
            .iter()
            .filter(|r| r.chance >= contract.min_chance as f32 && r.result == "failure")
            .count();

        let amount = (success * 900000 + failed_counted * 1000000) as u64;

        per_faction_rewards
            .entry(*faction_id)
            .or_default()
            .push((
                amount,
                format!(
                    "* **{} [{}]** - ${} (s: {}, f: {})",
                    player_data.name,
                    player_id,
                    format_with_commas(amount),
                    success,
                    failed_counted
                ),
            ));
    }

    let mut factions: Vec<(u64, u64, u64, Vec<(u64, String)>)> = per_faction_rewards
        .into_iter()
        .filter(|(_, players)| !players.is_empty())
        .map(|(faction_id, mut players)| {
            players.sort_by(|a, b| b.0.cmp(&a.0));
            let base_total: u64 = players.iter().map(|(amount, _)| *amount).sum();
            let final_total =
                (base_total as f64 * (1.0 + contract.faction_cut as f64 / 100.0)) as u64;
            (faction_id, base_total, final_total, players)
        })
        .collect();

    factions.sort_by(|a, b| b.2.cmp(&a.2));

    for (faction_id, base_total, final_total, players) in factions {
        let faction_label = faction_names
            .get(&faction_id)
            .cloned()
            .unwrap_or_else(|| faction_id.to_string());

        let header = format!(
            "**{}**\nEarned: ${} | Final (+{}%): ${}\n",
            faction_label,
            format_with_commas(base_total),
            contract.faction_cut,
            format_with_commas(final_total),
        );

        let lines: Vec<String> = players.iter().map(|(_, line)| line.clone()).collect();
        let page_descriptions = paginate_reward_descriptions(&header, &lines, &faction_label);
        let total_pages = page_descriptions.len();

        for (i, description) in page_descriptions.iter().enumerate() {
            let title = rewards_embed_title(&faction_label, i + 1, total_pages);

            let embed = CreateEmbed::new()
                .title(title)
                .description(description);

            ctx.channel_id()
                .send_message(ctx.serenity_context(), CreateMessage::new().embed(embed))
                .await?;
        }
    }

    Ok(())
}

fn format_with_commas(number: u64) -> String {
    let mut chars: Vec<_> = number.to_string().chars().collect();
    let len = chars.len();
    for i in (1..len).rev() {
        if (len - i) % 3 == 0 {
            chars.insert(i, ',');
        }
    }
    chars.into_iter().collect()
}

// https://discord.com/developers/docs/resources/message#embed-object-embed-limits
const EMBED_DESCRIPTION_LIMIT: usize = 4096;
const EMBED_TOTAL_LIMIT: usize = 6000;
const EMBED_SAFETY_BUFFER: usize = 512;

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn rewards_embed_title(faction_label: &str, page: usize, total_pages: usize) -> String {
    if total_pages <= 1 {
        format!("Rewards — {faction_label}")
    } else {
        format!("Rewards — {faction_label} ({page}/{total_pages})")
    }
}

fn embed_description_budget(title: &str) -> usize {
    EMBED_DESCRIPTION_LIMIT
        .min(EMBED_TOTAL_LIMIT.saturating_sub(char_len(title)))
        .saturating_sub(EMBED_SAFETY_BUFFER)
}

fn paginate_reward_descriptions(
    header: &str,
    lines: &[String],
    faction_label: &str,
) -> Vec<String> {
    if lines.is_empty() {
        return vec![header.trim_end().to_string()];
    }

    // Size pages against the longest plausible title so real titles always fit.
    let sizing_title = rewards_embed_title(faction_label, 999, 999);
    let max_desc = embed_description_budget(&sizing_title);

    let mut pages: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for line in lines {
        let trial = if current.is_empty() {
            format!("{header}{line}")
        } else {
            format!("{header}{}\n{line}", current.join("\n"))
        };

        if current.is_empty() || char_len(&trial) <= max_desc {
            current.push(line);
        } else {
            pages.push(format!("{header}{}", current.join("\n")));
            current = vec![line];
        }
    }

    if !current.is_empty() {
        pages.push(format!("{header}{}", current.join("\n")));
    }

    pages
}

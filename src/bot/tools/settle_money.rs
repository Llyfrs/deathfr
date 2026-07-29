use crate::database::structures::{Contract, ReviveEntry};
use crate::database::Database;
use crate::pricing::{classify_revive, ReviveClass};
use mongodb::bson::{doc, Bson};

pub async fn settle_contract_money(
    contract: &Contract,
    reviving_faction_ids: &[u64],
) -> anyhow::Result<u64> {
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
    .await?;

    let success_rate = contract.pricing_type.success_rate();
    let failed_rate = contract.pricing_type.failed_rate();

    let mut updated = 0u64;

    for revive in &revives {
        let amount = match classify_revive(revive, contract.min_chance) {
            ReviveClass::Success => success_rate,
            ReviveClass::FailedCounted => failed_rate,
            ReviveClass::Ignored => 0,
        };

        Database::update_doc::<ReviveEntry>(
            doc! { "id": &revive.id },
            doc! { "$set": { "money_made": amount as i64 } },
        )
        .await?;

        updated += 1;
    }

    Ok(updated)
}

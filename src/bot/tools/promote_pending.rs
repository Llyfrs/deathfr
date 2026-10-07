use crate::database::structures::{Contract, Status};
use crate::database::Database;
use chrono::Utc;
use mongodb::bson;
use mongodb::bson::doc;

/// Flips pending contracts whose start time has passed to active.
/// Returns how many contracts were promoted.
pub async fn promote_pending_contracts() -> anyhow::Result<usize> {
    let pending_contracts = Database::get_collection_with_filter::<Contract>(Some(
        doc! {"status": bson::to_bson(&Status::Pending)?},
    ))
    .await?;

    let now = Utc::now().timestamp() as u64;
    let mut promoted = 0;

    for mut contract in pending_contracts {
        if contract.started <= now {
            contract.status = Status::Active;
            Database::update(
                contract.clone(),
                doc! {"contract_id": contract.contract_id.clone()},
            )
            .await?;
            promoted += 1;
        }
    }

    Ok(promoted)
}

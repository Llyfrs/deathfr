mod api_key;
mod colection_name;
mod contract;
mod database_name;
mod player;
mod player_cache;
mod revive;
mod verification;

pub use api_key::APIKey;
pub use contract::Contract;
pub use contract::Status;
pub use player::Player;
pub use player_cache::PlayerCache;
pub use revive::ReviveEntry;
pub use revive::TargetLastAction;
pub use verification::Verification;

pub use colection_name::CollectionName;
pub use database_name::DatabaseName;

use mongodb::{error::Result, Client};

#[async_trait::async_trait]
pub trait IndexSetup {
    async fn ensure_indexes(client: &Client) -> Result<()>;
}

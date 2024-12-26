use crate::database::structures::{CollectionName, DatabaseName};
use mongodb::bson::doc;
use mongodb::{error::Result, Client};
use once_cell::sync::Lazy;
use serenity::futures::TryStreamExt;
use tokio::sync::Mutex;
pub struct Database;
static DB_CONN: Lazy<Mutex<Option<Client>>> = Lazy::new(|| Mutex::new(None));
static CONNECTION_URL: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

impl Database {
    pub async fn init(connection_url: String) -> Result<()> {
        // Store the connection URL in the static variable
        let mut url = CONNECTION_URL.lock().await;
        *url = Some(connection_url.clone());

        let mut db_conn = DB_CONN.lock().await; // Use `await` for the async mutex
        if db_conn.is_none() {
            let client = Client::with_uri_str(&connection_url).await?;
            *db_conn = Some(client);
        }
        Ok(())
    }

    pub async fn get() -> Option<Client> {
        let mut db_conn = DB_CONN.lock().await; // Use `await` for the async mutex

        if db_conn.is_none() {
            // Try reconnecting if no client exists
            let url = CONNECTION_URL.lock().await.clone();
            if let Some(connection_url) = url {
                let client = Client::with_uri_str(&connection_url).await.ok()?;
                *db_conn = Some(client);
            } else {
                // Instead of directly constructing InvalidArgument, use ErrorKind::msg
                return None;
            }
        }
        // Return the client if it's valid
        Some(db_conn.clone().unwrap())
    }
    pub async fn close() {
        let mut db_conn = DB_CONN.lock().await; // Use `await` for the async mutex
        if let Some(client) = db_conn.take() {
            client.shutdown().await;
        }
    }
    pub async fn get_collection<T>() -> Result<Vec<T>>
    where
        T: CollectionName
            + serde::de::DeserializeOwned
            + Unpin
            + 'static
            + DatabaseName
            + Sync
            + Send,
    {
        Database::get_collection_with_filter(None).await
    }

    pub async fn get_collection_with_filter<T>(
        filter: Option<mongodb::bson::Document>,
    ) -> Result<Vec<T>>
    where
        T: CollectionName
            + serde::de::DeserializeOwned
            + Unpin
            + 'static
            + DatabaseName
            + Sync
            + Send,
    {
        // Get the database client
        let client = Database::get().await.unwrap();

        // Use the database name from the trait or fall back to "deathfr"
        let db_name = T::database_name();
        let db = client.database(db_name);

        // Get the collection name and fetch the documents
        let collection_name = T::collection_name();
        let collection = db.collection::<T>(&collection_name);

        // Default to an empty filter if None is provided
        let filter = filter.unwrap_or_else(|| doc! {});

        let cursor = collection.find(filter).await?;
        let results: Vec<T> = cursor.try_collect().await?;

        Ok(results)
    }
}

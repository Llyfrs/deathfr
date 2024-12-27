use crate::database::structures::{CollectionName, DatabaseName};
use mongodb::bson::{doc, Document};
use mongodb::error::{ErrorKind, WriteFailure};
use mongodb::{error::Result, Client, Collection};
use once_cell::sync::Lazy;
use serde_json::{from_str, to_string};
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

    pub async fn insert_manny<T>(documents: Vec<T>) -> Result<()>
    where
        T: CollectionName + serde::Serialize + Unpin + 'static + DatabaseName + Sync + Send,
    {
        // Get the database client
        let client = Database::get().await.unwrap();

        // Use the database name from the trait or fall back to "deathfr"
        let db_name = T::database_name();
        let db = client.database(db_name);

        let collection_name = T::collection_name();
        let collection = db.collection::<T>(&collection_name);

        match collection.insert_many(documents).with_options(
            mongodb::options::InsertManyOptions::builder()
                .ordered(false) // If one fails, continue with the rest
                .build(),
        ).await {
            Ok(_) => Ok(()),
            Err(e) => {
                if let ErrorKind::InsertMany(ref insert_error) = *e.kind {
                    if let Some(write_errors) = &insert_error.write_errors {

                        log::error!("Write errors: {:?}", write_errors);

                        // Check if all errors are duplicate key errors (code 11000)
                        if write_errors.iter().all(|err| err.code == 11000) {
                            return Ok(()); // Ignore duplicate key errors
                        }
                    }
                }
                Err(e.into()) // Propagate other errors
            }
        }
    }

    pub async fn insert<T>(document: T) -> Result<()>
    where
        T: CollectionName + serde::Serialize + Unpin + 'static + DatabaseName + Sync + Send,
    {
        // Get the database client
        let client = Database::get().await.unwrap();

        // Use the database name from the trait or fall back to "deathfr"
        let db_name = T::database_name();
        let db = client.database(db_name);

        // Get the collection name and fetch the documents
        let collection_name = T::collection_name();
        let collection = db.collection::<T>(&collection_name);

        // The ides here are
        // to ignore duplicate key errors
        // as they aren't errors that need handling or results in a failure
        match collection.insert_one(document).await {
            Ok(_) => Ok(()),
            Err(e) => match *e.kind {
                ErrorKind::Write(WriteFailure::WriteError(ref err)) if err.code == 11000 => Ok(()), // Ignore duplicate key errors
                ErrorKind::Write(_) => Err(e.into()), // Handle other write errors
                _ => Err(e.into()),                   // Handle other kinds of errors
            },
        }
    }

    pub async fn set_value<T>(key: &str, value: T) -> Result<()>
    where
        T: serde::Serialize,
    {
        let client = Database::get().await.unwrap();
        let db = client.database("deathfr");
        let collection: Collection<Document> = db.collection("secrets");

        // Serialize the value into a string
        let serialized_value = to_string(&value).unwrap();

        let filter = doc! { "key": key };
        let update = doc! { "$set": doc! { "value": serialized_value } };

        // Use the `upsert` option to insert the document if it doesn't exist
        collection.update_one(filter, update).upsert(true).await?;

        Ok(())
    }
    pub async fn get_value<T>(key: &str) -> Option<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let client = Database::get().await.unwrap();
        let db = client.database("deathfr");
        let collection: Collection<Document> = db.collection("secrets");

        let filter = doc! { "key": key };
        let doc = collection.find_one(filter).await.unwrap();

        match doc {
            Some(doc) => {
                if let Some(value) = doc.get_str("value").ok() {
                    let deserialized_value: T = from_str(value).unwrap();
                    Some(deserialized_value)
                } else {
                    None
                }
            }
            None => None,
        }
    }
}

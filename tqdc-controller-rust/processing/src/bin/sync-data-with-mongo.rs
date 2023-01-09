use dataforge::read_df_header_and_meta;
use mongodb::{bson::{doc, Document, to_bson}, options::ClientOptions, Client};


#[tokio::main]
async fn main() -> mongodb::error::Result<()> {

    // Parse your connection string into an options struct
    let mut client_options =
        ClientOptions::parse("mongodb://localhost:27017")
            .await?;
    // Manually set an option
    client_options.app_name = Some("Rust Demo".to_string());
    // Get a handle to the cluster
    let client = Client::with_options(client_options)?;
    // Ping the server to see if you can connect to the cluster
    client
        .database("admin")
        .run_command(doc! {"ping": 1}, None)
        .await?;

    
    let db = client.database("numass");
    let collection = db.collection::<Document>("numass");

    let mut file = tokio::fs::File::open(
        "./test-data/points/p-2022-real-18600ev-30s.df"
    ).await.unwrap();

    let (_, meta) = read_df_header_and_meta::<serde_json::Value>(&mut file).await.unwrap();
    println!("{:?}", meta);

    let ser = match to_bson(&meta)? {
        mongodb::bson::Bson::Document(doc) => {doc}
        _ => unreachable!()
    };

    collection.insert_one(ser, None).await?;

    Ok(())
}
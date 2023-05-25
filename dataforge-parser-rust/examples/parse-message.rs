#[cfg(not(feature = "tokio"))]
fn main() {

    use dataforge::read_df_message_sync;

    let mut file = std::fs::File::open(
        "./resources/test/df01-point.df"
    ).unwrap();

    let msg = read_df_message_sync::<serde_json::Value>(&mut file).unwrap();
    println!("{:?}", msg.meta)
}

#[cfg(feature = "tokio")]
#[tokio::main]
async fn main() {

    use dataforge::read_df_message;

    let mut file = tokio::fs::File::open(
        "./resources/test/df01-point.df"
    ).await.unwrap();

    let msg = read_df_message::<serde_json::Value>(&mut file).await.unwrap();
    println!("{:?}", msg.meta)
}
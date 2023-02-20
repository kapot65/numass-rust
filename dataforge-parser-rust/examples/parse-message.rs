use dataforge::read_df_message;

#[cfg(not(feature = "tokio"))]
fn main() {
    let mut file = std::fs::File::open(
        "./resources/test/df01-point.df"
    ).unwrap();

    let msg = read_df_message::<serde_json::Value>(&mut file).unwrap();
    println!("{:?}", msg.meta)
}

#[cfg(feature = "tokio")]
#[tokio::main]
async fn main() {

    let mut file = tokio::fs::File::open(
        "./resources/test/df01-point.df"
    ).await.unwrap();

    let msg = read_df_message::<serde_json::Value>(&mut file).await.unwrap();
    println!("{:?}", msg.meta)
}
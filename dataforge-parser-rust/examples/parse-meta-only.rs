#[cfg(not(feature = "tokio"))]
fn main() {

    use dataforge::read_df_header_and_meta_sync;

    let mut file = std::fs::File::open(
        "./resources/test/df01-point.df"
    ).unwrap();

    let (_, meta ) = read_df_header_and_meta_sync::<serde_json::Value>(&mut file).unwrap();
    println!("{meta:?}")
}

#[cfg(feature = "tokio")]
#[tokio::main]
async fn main() {

    use dataforge::read_df_header_and_meta;

    let mut file = tokio::fs::File::open(
        "./resources/test/df01-point.df"
    ).await.unwrap();

    let (_, meta ) = read_df_header_and_meta::<serde_json::Value>(&mut file).await.unwrap();
    println!("{meta:?}")
}
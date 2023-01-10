use dataforge::read_df_header_and_meta;

#[tokio::main]
async fn main() {

    let mut file = tokio::fs::File::open(
        "./resources/test/df01-point.df"
    ).await.unwrap();

    let (_, meta ) = read_df_header_and_meta::<serde_json::Value>(&mut file).await.unwrap();
    println!("{:?}", meta)
}
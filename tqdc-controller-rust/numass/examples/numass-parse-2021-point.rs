use dataforge::read_df_message;
use numass::NumassMeta;

#[tokio::main]
async fn main() {

    let mut file = tokio::fs::File::open(
        "./test-data/points/p0(30s)(HV1=14000).df"
    ).await.unwrap();

    let msg = read_df_message::<NumassMeta>(&mut file).await.unwrap();
    println!("{:?}", msg.meta)
}
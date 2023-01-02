use dataforge::read_df_message;
use numass::NumassMeta;

#[tokio::main]
async fn main() {

    let mut file = tokio::fs::File::open(
        "./test-data/points/p-2022-11-14-real-5v.df"
    ).await.unwrap();

    let msg = read_df_message::<NumassMeta>(&mut file).await.unwrap();
    println!("{:?}", msg.meta)
}
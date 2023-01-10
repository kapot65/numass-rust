use protobuf::Message;

use dataforge::read_df_message;
use numass::{NumassMeta, protos::rsb_event::Point};

#[tokio::main]
async fn main() {

    let mut file = tokio::fs::File::open(
        "./resources/test/2022-point.df"
    ).await.unwrap();

    let msg = read_df_message::<NumassMeta>(&mut file).await.unwrap();

    let point = Point::parse_from_bytes(&msg.data.unwrap()[..]).unwrap();

    println!("{:?}", point);
    println!("{:?}", msg.meta);
}
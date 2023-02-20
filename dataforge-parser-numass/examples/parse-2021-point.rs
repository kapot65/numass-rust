use dataforge::read_df_message;
use numass::{NumassMeta, protos::rsb_event::Point};

use protobuf::Message;

fn main() {

    let mut file = std::fs::File::open(
        "resources/test/2021-point.df"
    ).unwrap();

    let msg = read_df_message::<NumassMeta>(&mut file).unwrap();

    let point = Point::parse_from_bytes(&msg.data.unwrap()[..]).unwrap();

    println!("{point:?}", );
    println!("{:?}", msg.meta);
}
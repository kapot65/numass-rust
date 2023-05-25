use processing::{numass::{protos::rsb_event, NumassMeta}, Algorithm, post_process, PostProcessingParams};
use protobuf::Message;


#[tokio::main]
async fn main() {

    let filepath = "/data/numass-server/2023_03/Tritium_5/set_1/p118(30s)(HV1=12000)";

    let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
    let message = dataforge::read_df_message::<NumassMeta>(&mut point_file)
        .await
        .unwrap();

    let point =
        rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

    let algorithm = Algorithm::default();
    let amlitudes = processing::extract_amplitudes(&point, &algorithm, true);

    let frames = amlitudes.len();

    let amps = post_process(
        amlitudes, 
        &PostProcessingParams::default());

    let counts = amps.iter().map(|(_, frames)| {
        frames.values().count()
    }).sum::<usize>();


    println!("counts: {counts}");
    println!("frames: {frames}");
}
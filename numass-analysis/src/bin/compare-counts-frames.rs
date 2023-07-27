use processing::{numass::{protos::rsb_event, NumassMeta}, post_process, PostProcessParams, ProcessParams};
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

    let processing = ProcessParams::default();
    let amlitudes = processing::extract_amplitudes(&point, &processing);

    let frames = amlitudes.len();

    let amps = post_process(
        amlitudes, 
        // &PostProcessingParams::default()

        &PostProcessParams {
            merge_close_events: false,
            use_dead_time: false,
            effective_dead_time: 4000,
            merge_map: [
                [false, true, false, false, false, false, false],
                [false, false, false, true, false, false, false],
                [false, false, false, false, true, false, false],
                [false, false, false, false, false, false, true],
                [true, false, false, false, false, false, false],
                [true, true, true, true, true, false, true],
                [false, false, true, false, false, false, false],
            ],
        }
    
    );

    let counts = amps.values().map(|frames| {
        frames.values().count()
    }).sum::<usize>();


    println!("counts: {counts}");
    println!("frames: {frames}");
}
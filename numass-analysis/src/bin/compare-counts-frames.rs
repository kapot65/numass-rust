use std::path::PathBuf;

use analysis::amps::get_amps;
use processing::{postprocess::{post_process, PostProcessParams}, process::ProcessParams};

#[tokio::main]
async fn main() {

    let filepath = PathBuf::from("/data/numass-server/2023_03/Tritium_5/set_1/p118(30s)(HV1=12000)");

    let amlitudes = get_amps(&filepath, &ProcessParams::default()).await.unwrap();

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
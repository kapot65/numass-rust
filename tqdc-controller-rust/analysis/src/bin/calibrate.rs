#[tokio::main]
async fn main() {

    use {
        std::{collections::HashMap, sync::Arc},
    
        protobuf::Message,
        tokio::sync::Mutex,
    
        numass::{protos::rsb_event, NumassMeta},
        processing::{histogram::PointHistogram, waveform_to_event, Algorithm}
    };
    

    let algorithm = Algorithm::Max;
    // let algorithm = Algorithm::Likhovid { left: 6, right: 36 };

    let points = [
        (
            6.0,
            "/data/numass-server/2022_12/Electrode_4/set_1/p2(200s)(HV1=6000)",
        ),
        (#[cfg(not(target_arch = "wasm32"))]
            12.0,
            "/data/numass-server/2022_12/Electrode_4/set_1/p5(200s)(HV1=12000)",
        ),
        (
            14.0,
            "/data/numass-server/2022_12/Electrode_4/set_1/p6(200s)(HV1=14000)",
        ),
        (
            16.0,
            "/data/numass-server/2022_12/Electrode_4/set_1/p7(200s)(HV1=16000)",
        ),
        (
            18.0,
            "/data/numass-server/2022_12/Electrode_4/set_1/p8(200s)(HV1=18000)",
        ),
        (
            20.0,
            "/data/numass-server/2022_12/Electrode_4/set_1/p9(200s)(HV1=20000)",
        ),
    ];

    let calibration_data: Arc<Mutex<HashMap<u8, Vec<_>>>> = Arc::new(Mutex::new(HashMap::new()));
    {
        let handles = points
            .iter()
            .map(|(kev, filepath)| {
                let kev = kev.to_owned();
                let filepath = filepath.to_owned();
                let calibration_data = Arc::clone(&calibration_data);
                tokio::spawn(async move {
                    let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
                    let message = dataforge::read_df_message::<NumassMeta>(&mut point_file)
                        .await
                        .unwrap();

                    let mut histogram = PointHistogram::new((0.0, 400.0), 400);

                    let point =
                        rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
                    for channel in &point.channels {
                        let amplitudes = channel
                            .blocks
                            .iter()
                            .flat_map(|block| {
                                block.frames.iter().map(|frame| {
                                    let waveform = processing::frame_to_waveform(frame);
                                    let (_, y) = waveform_to_event(&waveform, &algorithm);
                                    y
                                })
                            })
                            .collect::<Vec<_>>();
                        histogram.add_batch(channel.id as u8, amplitudes)
                    }

                    for (ch_id, y) in histogram.channels {
                        let (x, _) = y
                            .clone()
                            .iter()
                            .enumerate()
                            .max_by_key(|(_, amp)| **amp as i64 * 1000)
                            .unwrap();

                        {
                            let mut lock = calibration_data.lock().await;
                            let entry = lock.entry(ch_id).or_default();
                            entry.push((kev as f32, histogram.x[x]));
                        }
                    }
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle.await.unwrap();
        }
    }

    let calibration_data = calibration_data.lock().await.clone();

    let mut plot = plotly::Plot::new();

    let layout = plotly::Layout::new()
        .title(plotly::common::Title::new("calibration data"))
        .height(1000);
    plot.set_layout(layout);

    let coeffs = (0..7)
        .map(|ch_id| {
            let coeffs = &calibration_data[&ch_id];

            let (x, y): (Vec<_>, Vec<_>) = coeffs.iter().cloned().unzip();

            let (a, b): (f32, f32) = linreg::linear_regression(&y, &x).unwrap();

            let trace = plotly::Scatter::new(x, y).mode(plotly::common::Mode::Markers);

            plot.add_trace(trace);

            [a, b]
        })
        .collect::<Vec<_>>();

    plot.show();

    println!("{coeffs:#?}");
}

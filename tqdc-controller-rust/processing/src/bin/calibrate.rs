use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use processing::{waveform_to_event, Algorithm, histogram::PointHistogram};
use protobuf::Message;

use numass::protos::rsb_event;

#[tokio::main]
async fn main() {

    let algorithm = Algorithm::Likhovid { left: 6, right: 36 };
    let polyfit_range = 8usize;


    let points = [
        (6.0, "/data/numass-server/2022_12/Electrode_4/set_1/p2(200s)(HV1=6000)"),
        (8.0, "/data/numass-server/2022_12/Electrode_4/set_1/p3(200s)(HV1=8000)"),
        (10.0, "/data/numass-server/2022_12/Electrode_4/set_1/p4(200s)(HV1=10000)"),
        (12.0, "/data/numass-server/2022_12/Electrode_4/set_1/p5(200s)(HV1=12000)"),
        (14.0, "/data/numass-server/2022_12/Electrode_4/set_1/p6(200s)(HV1=14000)"),
        (16.0, "/data/numass-server/2022_12/Electrode_4/set_1/p7(200s)(HV1=16000)"),
        (18.0, "/data/numass-server/2022_12/Electrode_4/set_1/p8(200s)(HV1=18000)"),
        (20.0, "/data/numass-server/2022_12/Electrode_4/set_1/p9(200s)(HV1=20000)"),
    ];

    let calibration_data: Arc<Mutex<HashMap<u8, Vec<_>>>> = Arc::new(Mutex::new(HashMap::new()));
    {

        let handles = points.iter().map(|(kev, filepath)| {
            let kev = kev.to_owned();
            let filepath = filepath.to_owned();
            let calibration_data = Arc::clone(&calibration_data);
            tokio::spawn(async move {
                let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
                let message = numass::extract_df_message(&mut point_file).await.unwrap();

                let mut histogram = PointHistogram::new((0.0, 100.0), 1000);

                let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
                for channel in &point.channels {
                    let amplitudes = channel.blocks.iter().flat_map(|block| {
                        block.frames.iter().map(|frame| {
                            let waveform = processing::frame_to_waveform(frame);
                            let (_, y) = waveform_to_event(&waveform, &algorithm);
                            y
                        })
                    }).collect::<Vec<_>>();
                    histogram.add_batch(channel.id as u8, amplitudes)
                }

                // let mut plot = plotly::Plot::new();

                // let layout = plotly::Layout::new()
                // .height(1000);
                // plot.set_layout(layout);


                for (ch_id, y) in histogram.channels {
    
                    let (x, _)  = y.clone().iter().enumerate().max_by_key(|(_, amp)| **amp as u64).unwrap();
                    
                    // let coeffs = polyfit_rs::polyfit_rs::polyfit(
                    //     &histogram.x[x-polyfit_range..x+polyfit_range], 
                    //     &y[x-polyfit_range..x+polyfit_range], 2).unwrap();
                    // let a = coeffs[2];
                    // let b = coeffs[1];
                    // let c = coeffs[0];

                    // let peak_x = -b / (2.0 * a);

                    {
                        let mut lock = calibration_data.lock().await;
                        let entry = lock.entry(ch_id).or_default();
                        entry.push((kev as f32, histogram.x[x] as f32));
                    }

                    // println!("ch #{ch_id} {coeffs:?}");
                    // println!("{}", -b / (2.0 * a));

                    // let x_poly = histogram.x[x-polyfit_range..x+polyfit_range].to_vec();
                    // let y_poly = histogram.x[x-polyfit_range..x+polyfit_range].iter().map(|x| a * x * x + b * x + c).collect::<Vec<_>>();

                    // let trace = plotly::Scatter::new(x_poly, y_poly)
                    // .name(format!("ch #{ch_id}"));

                    // plot.add_trace(trace);

                    // let trace = plotly::Scatter::new(histogram.x.to_owned(), y.to_owned())
                    // .line(plotly::common::Line::new().shape(plotly::common::LineShape::Hvh))
                    // .name(format!("ch #{ch_id}"));

                    // plot.add_trace(trace);
                }

                    // plot.show();
            })
        }).collect::<Vec<_>>();

        for handle in handles {
            handle.await.unwrap();
        }
    }

    let calibration_data = {
        let mut calibration_data = calibration_data.lock().await.clone();

        // for (_, coeffs) in calibration_data {
        //     coeffs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        // }
        
        calibration_data
    };

    let mut plot = plotly::Plot::new();

    let layout = plotly::Layout::new()
    .title(plotly::common::Title::new("calibration data"))
    .height(1000);
    plot.set_layout(layout);

    let coeffs = (0..7).map(|ch_id| {
        let coeffs = &calibration_data[&ch_id];

        let (x, y): (Vec<_>, Vec<_>) = coeffs.iter().cloned().unzip();

        let (a, b): (f32, f32) = linreg::linear_regression(&y, &x).unwrap();

        let trace = plotly::Scatter::new(x, y)
        .mode(plotly::common::Mode::Markers);

        plot.add_trace(trace);

        [a, b]
    }).collect::<Vec<_>>();

    plot.show();

    println!("{coeffs:#?}");
}
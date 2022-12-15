
use plotters::prelude::*;
use protobuf::Message;

use dataforge::protos::rsb_event;


#[derive(Debug, Clone)]
struct WaveformNormed {
    waveform: Vec<i16>,
    baseline: f32,
    bin: usize,
    x: f32,
    y: f32
}

// TODO move to lib
fn frame_to_waveform_normed(frame: &rsb_event::point::channel::block::Frame) -> Vec<i16>{

    let waveform_len = frame.data.len() / 2;
    (0..waveform_len).map(|idx| {
        i16::from_le_bytes(frame.data[idx*2..idx*2+2].try_into().unwrap())
    }).collect::<Vec<_>>()
}

fn correct_amp(y0: f32, y1: f32, y2: f32) -> (f32, f32) {
    (
        // calculated with SymPy
        (y0 - y2)/(2.0*(y0 - 2.0*y1 + y2)),
        (-(y0*y0)/8.0 + y0*y1 + y0*y2/4.0 - 2.0 * y1 * y1 + y1*y2 - (y2*y2)/8.0)/(y0 - 2.0 * y1 + y2)
    )
}

#[tokio::main]
async fn main() {

    let filepath = "/data/2022_12/Electrode_4/set_1/p4(200s)(HV1=10000)";
    // let filepath = "/data/2022_12/Tritium_5/set_8/p121(30s)(HV1=14000)";

    let threshold = 20.0;
    let step = 10.0;

    let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
    let message = dataforge::extract_df_message(&mut point_file).await.unwrap();

    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

    let waveforms_ch6 = point.channels.iter().find(|ch| ch.id == 5).unwrap().blocks[0].frames.iter()
        .map(|frame| {

            let waveform = frame_to_waveform_normed(frame);

            let baseline = waveform.iter().take(16).sum::<i16>() as f32 / 16.0;

            let bin = waveform.iter().enumerate().find(|(idx, amp)| {
                let amp = **amp as f32 - baseline;
                amp > threshold &&
                (*idx == 0 || waveform[idx - 1] as f32 - baseline <= amp)&&
                (*idx == waveform.len() - 1 || waveform[idx + 1] as f32 - baseline <= amp)
            }).map(|(idx, _)| idx).unwrap();


            let left = if bin == 0 {
                waveform[bin + 1] as f32
            } else {
                waveform[bin - 1] as f32
            };
            let center = waveform[bin] as f32;
            let right = if bin == waveform.len() - 1 {
                left
            } else {
                waveform[bin + 1] as f32
            };


            let (x, y) = correct_amp(left, center, right);

            WaveformNormed {
                waveform,
                baseline,
                bin,
                x,
                y
            }
    });

    let mut groups = vec![vec![]; 40];

    waveforms_ch6.for_each(|wf| {
        let group = (wf.y - wf.baseline) / step;
        if group < 40.0 {
            groups[group as usize].push(wf);
        }
    });


    let handles =  groups.iter().enumerate().map(|(idx, group)| {

        let group = group.clone();
        let filepath = filepath.to_owned();
        tokio::spawn(async move {

            let amp_min = idx as f32 * step;
            let amp_max = (idx as f32 + 1.0) * step;

            let caption = format!("{filepath} {} - {} : {} events", amp_min, amp_max, group.len());

            let filename = format!("imgs/{idx}.png");

            let root = BitMapBackend::new(&filename, (1920, 1080)).into_drawing_area();
            root.fill(&WHITE).unwrap();
            let mut chart = ChartBuilder::on(&root)
                .caption(caption, ("sans-serif", 50).into_font())
                .margin(5)
                .x_label_area_size(50)
                .y_label_area_size(50)
                .build_cartesian_2d(-10f32..75f32, -20f32..400f32).unwrap();

            chart.configure_mesh().draw().unwrap();

            for WaveformNormed { waveform, baseline, bin, x, y } in group {

                let offset_x = bin as f32 - 35.0 + x;
                // let scale_y = y / 195.0;

                let x = (0..waveform.len()).map(|x| x as f32 - offset_x)
                    .collect::<Vec<_>>();

                let y = waveform.iter().map(|y| (*y as f32 - baseline ) /* / scale_y */ ).collect::<Vec<_>>();

                let vals = x.iter().zip(y.iter());

                chart
                .draw_series(LineSeries::new(
                    vals.map(|(x, y)| (*x, *y)).collect::<Vec<_>>(),
                    RED.to_rgba().mix(0.02)
                )).unwrap();
            }

            chart
                .configure_series_labels()
                .background_style(WHITE.mix(0.8))
                .border_style(BLACK)
                .draw().unwrap();

            root.present().unwrap();
            })
    }).collect::<Vec<_>>();


    for handle in handles {
        handle.await.unwrap();
    }
}
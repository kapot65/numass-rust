#[tokio::main]
async fn main() {
    use {
        dataforge::read_df_message,
        plotters::prelude::*,
        processing::{
            process_waveform, ProcessedWaveform,
            numass::{protos::rsb_event, NumassMeta},
            correct_amp, find_first_peak
        },
        protobuf::Message,
    };

    #[derive(Debug, Clone)]
    struct WaveformNormed {
        waveform: ProcessedWaveform,
        bin: usize,
        x: f32,
        y: f32,
    }

    // let filepath = "/data/2022_12/Electrode_4/set_1/p4(200s)(HV1=10000)";
    // let filepath = "/data/2022_12/Electrode_4/set_1/p4(200s)(HV1=10000)";
    let filepath = "/data/2022_12/Gun_16/set_1/p0(200s)(HV1=15990)";
    // let filepath = "/data/2022_12/Tritium_5/set_8/p121(30s)(HV1=14000)";
    let channel = 6;

    let threshold = 20.0;
    let step = 10.0;

    let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
    let message = read_df_message::<NumassMeta>(&mut point_file)
        .await
        .unwrap();

    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

    let waveforms_ch6 = point
        .channels
        .iter()
        .find(|ch| ch.id == channel - 1)
        .unwrap()
        .blocks[0]
        .frames
        .iter()
        .filter_map(|frame| {
            let waveform = process_waveform(frame);

            let bin = find_first_peak(&waveform, threshold);
            bin.map(|bin| {
                // mirror neighbors if peak is on the edge
                let left = if bin == 0 { waveform.0[bin + 1] } else { waveform.0[bin - 1] };
                let center = waveform.0[bin];
                let right = if bin == waveform.0.len() - 1 { left } else { waveform.0[bin + 1] };
    
                let (x, y) = correct_amp(left, center, right);
                WaveformNormed { waveform, bin, x, y }
            })
        });

    let mut groups = vec![vec![]; 40];

    waveforms_ch6.for_each(|wf| {
        let group = wf.y / step;
        if group < 40.0 {
            groups[group as usize].push(wf);
        }
    });

    let handles = groups
        .iter()
        .enumerate()
        .map(|(idx, group)| {
            let group = group.clone();
            let filepath = filepath.to_owned();
            tokio::spawn(async move {
                let amp_min = idx as f32 * step;
                let amp_max = (idx as f32 + 1.0) * step;

                let caption = format!(
                    "{filepath} (ch # {channel}) {amp_min} - {amp_max} : {} events",
                    group.len()
                );

                let filename = format!("imgs/{idx}.png");

                let root = BitMapBackend::new(&filename, (1920, 1080)).into_drawing_area();
                root.fill(&WHITE).unwrap();
                let mut chart = ChartBuilder::on(&root)
                    .caption(caption, ("sans-serif", 50).into_font())
                    .margin(5)
                    .x_label_area_size(50)
                    .y_label_area_size(50)
                    .build_cartesian_2d(-10f32..75f32, -20f32..400f32)
                    .unwrap();

                chart.configure_mesh().draw().unwrap();

                for WaveformNormed {
                    waveform,
                    bin,
                    x,
                    y: _,
                } in group
                {
                    let offset_x = bin as f32 - 35.0 + x;
                    // let scale_y = y / 195.0;

                    let x = (0..waveform.0.len())
                        .map(|x| x as f32 - offset_x)
                        .collect::<Vec<_>>();

                    let y = waveform.0.to_vec();

                    let vals = x.iter().zip(y.iter());

                    chart
                        .draw_series(LineSeries::new(
                            vals.map(|(x, y)| (*x, *y)).collect::<Vec<_>>(),
                            RED.to_rgba().mix(0.02),
                        ))
                        .unwrap();
                }

                chart
                    .configure_series_labels()
                    .background_style(WHITE.mix(0.8))
                    .border_style(BLACK)
                    .draw()
                    .unwrap();

                root.present().unwrap();
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.await.unwrap();
    }
}

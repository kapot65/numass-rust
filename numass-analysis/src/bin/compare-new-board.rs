use std::path::{Path, PathBuf};

use dataforge::read_df_message_sync;
use plotly::{Plot, Layout};
use processing::{numass::{NumassMeta, protos::rsb_event}, process::process_waveform, histogram::PointHistogram};
use protobuf::Message;

use unzip_n::unzip_n;

unzip_n!(2);

fn wafeform_to_amp<T>(waveform: &[T],  baseline: f32, coeff: f32) -> f32 
    where T: PartialOrd + Into<f32> + Clone,  
{
    const LEFT: i32 = 6;
    const RIGHT: i32 = 36;

    let (argmax, _) = waveform.iter().enumerate()
        .max_by(|(_, x), (_, y)| x.partial_cmp(y).unwrap())
        .unwrap();
    let argmax = argmax as i32;

    let left = 0_i32.max(argmax - LEFT) as usize;
    let right = (waveform.len() as i32).min(argmax + RIGHT) as usize;

    waveform[left..right].iter().map(|v| {
        let amp: f32 =  v.to_owned().into();
        (amp - baseline) * coeff
    }).sum::<f32>()
}

fn make_hist(point: &rsb_event::Point, ch_id: u64,  baseline: f32, coeff: f32) -> Vec<f32> {
    let mut amps = vec![];

    for channel in &point.channels {
        let block = &channel.blocks[0];
        for frame in &block.frames {

                if channel.id == ch_id {
                    let waveform = process_waveform(frame);
                    let amp = wafeform_to_amp(&waveform.0, baseline, coeff);
                    amps.push(amp)
                }
        }
    }

    amps
    // let frames = frames.iter().collect::<Vec<_>>();
}

fn point_to_amps<T: AsRef<Path>>(filepath: T, ch_id: u64, baseline: f32, coeff: f32) -> Vec<f32> {

    let mut point_file = std::fs::File::open(filepath).unwrap();
    let message = read_df_message_sync::<NumassMeta>(&mut point_file)
        .unwrap();
    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..])
        .unwrap();

    make_hist(&point, ch_id, baseline, coeff)
}

fn main() {

    let bin_size = 16.0;

    let data_root: PathBuf = PathBuf::from("/data/numass-server/");

    let old_amps = point_to_amps(
        data_root.join("2023_03/Tritium_1/set_1/p0(30s)(HV1=14000)"), 
        5, 0.0, 0.25
    );

    let new_amps_p1 = point_to_amps(
        data_root.join("new-board/p1(20s).df"), 
        0, 0.0, 0.75
    );

    let new_amps_p2 = point_to_amps(
        data_root.join("new-board/p2(20s)"), 
        0, 0.0, 0.588
    );

    let new_amps_p3 = point_to_amps(
        data_root.join("new-board/p3(20s)"), 
        0, 0.0, 0.441
    );

    let new_amps_p4 = point_to_amps(
        data_root.join("new-board/p4(20s)"), 
        0, 0.0, 0.441
    );
    
    let layout = Layout::new().height(1000);

    let mut plot = Plot::new();
    plot.set_layout(layout);

    let mut hist_old = PointHistogram::new_step(0.0..1000.0, bin_size);

    println!("{old_amps:?}");

    hist_old.add_batch(0, old_amps);

    

    let mut hist_new_p1 = PointHistogram::new_step(0.0..1000.0, bin_size);
    hist_new_p1.add_batch(0, new_amps_p1);

    let mut hist_new_p2 = PointHistogram::new_step(0.0..1000.0, bin_size);
    hist_new_p2.add_batch(0, new_amps_p2);

    let mut hist_new_p3 = PointHistogram::new_step(0.0..1000.0, bin_size);
    hist_new_p3.add_batch(0, new_amps_p3);

    let mut hist_new_p4 = PointHistogram::new_step(0.0..1000.0, bin_size);
    hist_new_p4.add_batch(0, new_amps_p4);


    hist_old.draw_plotly(&mut plot, Some("current board"));
    hist_new_p1.draw_plotly(&mut plot, Some("new board trapezium 4-2-4"));
    hist_new_p2.draw_plotly(&mut plot, Some("new board diff 12-12"));
    hist_new_p3.draw_plotly(&mut plot, Some("new board diff 12-12 (960 ns)"));
    hist_new_p4.draw_plotly(&mut plot, Some("new board diff 16-16 (960 ns)"));

    plot.show();
}
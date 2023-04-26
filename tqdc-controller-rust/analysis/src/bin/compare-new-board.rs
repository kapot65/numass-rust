use std::path::{Path, PathBuf};

use dataforge::read_df_message_sync;
use plotly::{Plot, Histogram, Layout, histogram::Bins, layout::BarMode};
use processing::{frame_to_waveform, numass::{NumassMeta, protos::rsb_event}, process_waveform};
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

    // let mut frames = BTreeMap::new();

    // const KERNEL: [f32; 10] = [
    //     -0.125f32, -0.125, -0.125, -0.125,
    //     0.0, 0.0,
    //     0.125, 0.125, 0.125, 0.125
    // ];

    
    for channel in &point.channels {
        let block = &channel.blocks[0];
        for frame in &block.frames {

                if channel.id == ch_id {
                    let waveform = process_waveform(&frame_to_waveform(frame));
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
    let opacity = 0.3;


    let data_root: PathBuf = PathBuf::from("/data/numass-server/");

    let old_amps = point_to_amps(
        data_root.join("2023_03/Tritium_1/set_1/p0(30s)(HV1=14000)"), 
        5, 44.4, 0.25
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
    
    
    let layout = Layout::new()
        .bar_mode(BarMode::Overlay)
        // .x_axis(Axis::new().title(Title::new("time delta, ns")))
        // .y_axis(Axis::new().type_(plotly::layout::AxisType::Log))
        // .height(1000)
        ;

    let mut plot = Plot::new();
    plot.set_layout(layout);

    {
        let trace = Histogram::new(old_amps)
            .x_bins(Bins::new(0.0, 8000.0, bin_size))
            .name("current board")
            .opacity(opacity);
        plot.add_trace(trace);
    }


    {
        let trace2 = Histogram::new(new_amps_p1)
            .x_bins(Bins::new(0.0, 4000.0, bin_size))
            .name("new board trapezium 4-2-4")
            .opacity(opacity);
        plot.add_trace(trace2);
    }


    {
        let trace3 = Histogram::new(new_amps_p2)
            .x_bins(Bins::new(0.0, 4000.0, bin_size))
            .name("new board diff 12-12")
            .opacity(opacity);
        plot.add_trace(trace3);
    }

    {
        let trace4 = Histogram::new(new_amps_p3)
            .x_bins(Bins::new(0.0, 4000.0, bin_size))
            .name("new board diff 12-12 (960 ns)")
            .opacity(opacity);
        plot.add_trace(trace4);
    }

    {
        let trace5 = Histogram::new(new_amps_p4)
            .x_bins(Bins::new(0.0, 4000.0, bin_size))
            .name("new board diff 16-16 (960 ns)")
            .opacity(opacity);
        plot.add_trace(trace5);
    }

    plot.show();
}
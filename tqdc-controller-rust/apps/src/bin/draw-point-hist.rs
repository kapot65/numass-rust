use clap::Parser;
use plotly::Plot;
use dataforge::protos::rsb_event;
use protobuf::Message;

/// Draws Acquisition point histogramm.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Opt {
    /// Path to point file
    #[clap()]
    filepath: String,

    #[clap(long, default_value_t = 0)]
    min: i32,

    #[clap(long, default_value_t = 400)]
    max: i32,

    #[clap(long, default_value_t = 400)]
    bins: usize
}

#[tokio::main]
async fn main() {

    let opt = Opt::parse();

    let mut point_file = tokio::fs::File::open(opt.filepath).await.unwrap();
    let message = dataforge::extract_df_message(&mut point_file).await.unwrap();

    let data = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

    let mut plot = Plot::new();

    for channel in &data.channels {

        let amplitudes = channel.blocks.iter().flat_map(|block| {
            block.frames.iter().map(|frame| {
                let waveform_len = frame.data.len() / 2;

                let waveform = (0..waveform_len).map(|idx| {
                    i16::from_le_bytes(frame.data[idx*2..idx*2+2].try_into().unwrap())
                });

                let first = waveform.clone().take(4).sum::<i16>() / 4;

                let waveform_normed = waveform.map(|ampl| {
                    ampl - first
                });

                waveform_normed.max().unwrap()
            })
        }).collect::<Vec<_>>();

        let step = (opt.max - opt.min) as f32 / opt.bins as f32;
        let bins_x = (0..opt.bins).map(|idx| {
            opt.min as f32 + step * (idx as f32) + step / 2.0
        }).collect::<Vec<f32>>();
        let mut bins = vec!(0; opt.bins as usize);
        for amplitude in amplitudes {
            let idx = (amplitude as i32 - opt.min) as f32 / step;
            if idx >= 0.0 && idx < opt.bins as f32 {
                bins[idx as usize] += 1;
            }
        };

        let trace = plotly::Scatter::new(bins_x, bins)
            .name(format!("ch-{}", channel.id))
            .line(plotly::common::Line::new().shape(
                plotly::common::LineShape::Hvh));
        plot.add_trace(trace);
    }

    plot.show(); 
}
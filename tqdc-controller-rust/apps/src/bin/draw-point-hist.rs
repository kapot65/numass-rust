use clap::Parser;
use plotly::Plot;
use protobuf::Message;

use dataforge::protos::rsb_event;
use apps::point_to_histogramm;

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

    let histogram = point_to_histogramm(&data, (opt.min, opt.max), opt.bins).await;
    let mut plot = Plot::new();

    for (ch_num, bins) in histogram.channels {
        let trace = plotly::Scatter::new(histogram.x.clone(), bins)
            .name(format!("ch-{}", ch_num))
            .line(plotly::common::Line::new().shape(
                plotly::common::LineShape::Hvh));
        plot.add_trace(trace);
    }

    plot.show(); 
}
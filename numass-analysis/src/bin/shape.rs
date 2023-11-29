use std::{sync::Arc, collections::BTreeMap};

use plotly::{common::{Title, ErrorData, ErrorType}, layout::Axis, Layout, Plot, Scatter};
use protobuf::Message;

use dataforge::read_df_message;
use processing::{
    histogram::{PointHistogram, HistogramParams}, numass::{protos::rsb_event, NumassMeta}, extract_events, ProcessParams, events_to_histogram
};
use tokio::sync::Mutex;

use statrs::statistics::Statistics;

#[tokio::main]
async fn main() {
    
    let db_root = "/data-nvme";
    let run = "2023_03";

    let pattern = format!("/{run}/Tritium_2/set_*/p*(HV1=14000)");
    // let pattern = format!("/{run}/Tritium_1/set_[123][1234567890]/p19(30s)(HV1=14000)");
    let exclude: Vec<String> = vec![];
    let points = analysis::get_points_by_pattern(db_root, &pattern, &exclude);

    // let mut points = BTreeMap::new();
    // points.insert(14000u16, vec![]);



    let range = 2.0..20.0;
    let bins = 180;

    let count_rates = Arc::new(Mutex::new(
        BTreeMap::new()
    ));

    let handles = points.iter().flat_map(|(_, filepaths)| {
        filepaths.iter().map(|filepath| {
            let range = range.clone();
            let bins = bins;
            let count_rates = Arc::clone(&count_rates);
            let filepath = filepath.clone();
            tokio::spawn(async move {

                let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
                let message = read_df_message::<NumassMeta>(&mut point_file)
                    .await
                    .unwrap();
                let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
                

                let hist = events_to_histogram(extract_events(
                    &point, 
                    &ProcessParams::default()
                ), HistogramParams {
                    range,
                    bins
                });

                let norm = hist.events_all(None) as f64;

                let mut count_rates = count_rates.lock().await;
                hist.merge_channels().into_iter().enumerate().for_each(|(idx, val)| {
                    let count_rate = val as f64 / norm;
                    count_rates.entry(idx).or_insert(Vec::new()).push(count_rate);
                });
            })
        })
    }).collect::<Vec<_>>();

    for handle in handles {
        handle.await.unwrap();
    }
    
    let histogram = PointHistogram::new(range, bins);

    let count_rates = count_rates.lock().await;

    let (mean, std): (Vec<_>, Vec<_>) = count_rates.values().map(|vector| {
        (vector.mean(), vector.std_dev())
    }).unzip();


    let shape = Scatter::new(histogram.x, mean)
        .name("shape")
        .error_y(ErrorData::new(ErrorType::Data).array(std));

    let mut plot = Plot::new();

    let layout = Layout::new()
        .title(Title::new(format!("Shape for {pattern}").as_str()))
        .x_axis(Axis::new().title(Title::new("U_sp, kV")))
        // .y_axis(Axis::new().type_(plotly::layout::AxisType::Log))
        .height(1000);

    plot.set_layout(layout);
    plot.add_trace(shape);

    plot.show();
}
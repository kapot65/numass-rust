use analysis::{
    get_points_by_pattern,
    workspace::{get_db_fast_root, get_workspace},
};
use processing::{process::TRAPEZOID_DEFAULT, types::FrameEvent};

use {
    processing::{histogram::PointHistogram, process::ProcessParams, storage::process_point},
    std::sync::Arc,
    tokio::sync::Mutex,
};

#[tokio::main]
async fn main() {
    let pattern = format!("{run}/Tritium_*/set_[1234]/p*", run = "2024_11");
    let exclude = vec![];

    // let u_sp = [12500, 13000, 13500, 14000, 14500, 15000, 15500, 16000, 16500, 17000];

    // let processing_params = ProcessParams {
    //     algorithm: TRAPEZOID_DEFAULT,
    //     convert_to_kev: false,
    // };

    // let hist = PointHistogram::new(60.0..150.0, 360);

    let u_sp = [
        12500, 13000, 13500, 14000, 14500, 15000, 15500, 16000, 16500, 17000,
    ];

    let processing_params = ProcessParams {
        algorithm: TRAPEZOID_DEFAULT,
        convert_to_kev: true,
    };

    let hist = PointHistogram::new(8.0..20.0, 240);

    for u_sp in u_sp {
        let points =
            get_points_by_pattern(get_db_fast_root().to_str().unwrap(), &pattern, &exclude);
        let points = points[&u_sp].clone();
        let pb = Arc::new(Mutex::new(indicatif::ProgressBar::new(points.len() as u64)));
        let histogram = Arc::new(Mutex::new(hist.clone()));

        let handles = points
            .iter()
            .map(|filepath| {
                let filepath = filepath.to_owned();
                let histogram = Arc::clone(&histogram);
                let processing_params = processing_params.clone();
                let pb = Arc::clone(&pb);

                tokio::spawn(async move {
                    // load point manually because it must not be used from cache
                    // TODO: add cache expiration based on calibration parameters change
                    let (_, events) = process_point(&filepath, &processing_params).await.unwrap();
                    let events = events.unwrap();
                    {
                        let mut histogram = histogram.lock().await;
                        for (_, events) in events {
                            for (_, event) in events {
                                if let FrameEvent::Event {
                                    channel, amplitude, ..
                                } = event
                                {
                                    histogram.add(channel, amplitude);
                                }
                            }
                        }
                    }
                    pb.lock().await.inc(1)
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle.await.unwrap();
        }

        {
            let histogram = histogram.lock().await;
            std::fs::create_dir_all(&get_workspace().join("calibrations")).unwrap();
            tokio::fs::write(
                get_workspace().join(format!("calibrations/{u_sp}.csv")),
                histogram.to_csv(','),
            )
            .await
            .unwrap();
        }
    }
}

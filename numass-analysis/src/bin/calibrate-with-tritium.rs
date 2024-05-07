use analysis::{get_points_by_pattern, workspace::{get_workspace, get_db_fast_root}};
use processing::types::FrameEvent;

use {
    processing::{
        histogram::PointHistogram,
        process::ProcessParams,
        storage::process_point,
    },
    std::sync::Arc,
    tokio::sync::Mutex,
};

#[tokio::main]
async fn main() {

    let pattern = format!("{run}/Tritium_*/set_[1234]/p*",
        run = "2023_03"
    );
    let exclude = vec![
        "Tritium_1/set_10".to_owned(),
        "Tritium_2/set_5/p18".to_owned(),
        "Tritium_2/set_5/p19".to_owned(),
        "Tritium_2/set_14/p20".to_owned(),
        "Tritium_2/set_14/p22".to_owned(),
        "Tritium_3/set_25_short".to_owned(),
        "Tritium_2/set_29/p37".to_owned(),
        "Tritium_4/set_10_short".to_owned(),
        "Tritium_4/set_25/p7".to_owned(),
        "Tritium_4/set_25_18000V_bad".to_owned(),
        "Tritium_5/set_10".to_owned()
    ];

    // let u_sp = [12000, 12500, 13000, 13500, 14000, 14500, 15000, 15500, 16000, 16500, 17000];
 
    // let processing_params = ProcessParams {
    //     algorithm: Algorithm::default(),
    //     convert_to_kev: false,
    // };

    // let hist = PointHistogram::new(0.0..120.0, 480);

    let u_sp = [12000, 12500, 13000, 13500, 14000, 14500, 15000, 15500, 16000, 16500, 17000];

    let processing_params = ProcessParams::default();

    let hist = PointHistogram::new(0.0..20.0, 400);
    
    for u_sp in u_sp {
        let points = get_points_by_pattern(get_db_fast_root().to_str().unwrap(), &pattern, &exclude);
        let points = points[&u_sp].clone();
        let pb = Arc::new(Mutex::new(indicatif::ProgressBar::new(points.len() as u64)));
        let histogram = Arc::new(Mutex::new(hist.clone()));

        let handles = points.iter().map(|filepath| {
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
                            if let FrameEvent::Event { channel, amplitude, .. } = event {
                                histogram.add(channel, amplitude);
                            }
                        }
                    }
                }
                pb.lock().await.inc(1) 
            })
        }).collect::<Vec<_>>();

        for handle in handles {
            handle.await.unwrap();
        }

        {
            let histogram = histogram.lock().await;
            std::fs::create_dir_all(&get_workspace().join("calibrations")).unwrap();
            tokio::fs::write(get_workspace().join(format!("calibrations/{u_sp}.csv")), histogram.to_csv(',')).await.unwrap();
        }
    }
}

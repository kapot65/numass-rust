use std::{path::PathBuf, collections::BTreeMap, vec, sync::Arc};

use analysis::{get_points_by_pattern, CorrectionCoeffs};
use dataforge::read_df_message;
use indicatif::ProgressStyle;
use processing::{Algorithm, numass::{NumassMeta, protos::rsb_event}, PostProcessParams, post_process, extract_events, ProcessParams, check_neigbors_fast};
use protobuf::Message;
use serde::{Serialize, Deserialize};
use tokio::sync::Mutex;

const PROCESSING: ProcessParams = ProcessParams {
    algorithm: Algorithm::FirstPeak { threshold: 15, left: 8 },
    convert_to_kev: true,
};

const POST_PROCESSING: PostProcessParams = PostProcessParams {
    merge_close_events: true,
    use_dead_time: false,
    effective_dead_time: 0,
    merge_map: [
        [false, true, false, false, false, false, false],
        [false, false, false, true, false, false, false],
        [false, false, false, false, true, false, false],
        [false, false, false, false, false, false, true],
        [true, false, false, false, false, false, false],
        [true, true, true, true, true, false, true],
        [false, false, true, false, false, false, false],
    ],
};


#[derive(Debug, Serialize, Deserialize, Clone)]
struct ProducedPoint {
    u_sp: u16,
    e_curr: f32,
    l_curr: f32,
    k: f64,
    l: f64,
    m: f64,
    d: f64,
    d_sum: f64,
    origins: Vec<PathBuf>,
    time: u64
}

#[tokio::main]
async fn main() {

    // let db_root = "/data/numass-server";
    // let db_root = "/data-ssd";
    let db_root = "/data-nvme";
    let run = "2023_03";

    let workspace = PathBuf::from("/home/chernov/Documents/produced/simple-spectrum");


    // === Tritium 2 ===
    let pattern = format!("/{run}/Tritium_2/set_*/p*");
    let exclude: Vec<String> = vec![
        "Tritium_2/set_5/p18".to_owned(),
        "Tritium_2/set_5/p19".to_owned(),
        "Tritium_2/set_14/p20".to_owned(),
        "Tritium_2/set_14/p22".to_owned(),
    ];
    let correct_to_monitor = true;
    let group = "tritium-2";

    // === End ===
    std::fs::create_dir_all(&workspace).unwrap();

    let bgr_dir = workspace.join(group);
    std::fs::create_dir_all(&bgr_dir).unwrap();

    let coeffs = Arc::new(CorrectionCoeffs::load(&format!("/{db_root}/monitor.json")));
    let points = get_points_by_pattern(db_root, &pattern, &exclude);

    let pb: indicatif::ProgressBar = indicatif::ProgressBar::new(points.len() as u64);
    pb.set_style(ProgressStyle::with_template("[{elapsed_precise}] {bar} {pos:>7}/{len:7} {msg}")
    .unwrap());
    let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::new(Mutex::new(pb));

    let table = Arc::new(Mutex::new(BTreeMap::new()));

    let handles = points.iter().map(|(u_sp, points)| {
        let u_sp_v = *u_sp;
        let points = points.clone();
        let table = Arc::clone(&table);
        let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::clone(&pb);
        let coeffs = Arc::clone(&coeffs);

        tokio::spawn(async move {
    
            let mut counts = 0.0;
            let mut time = 0.0;

            for filepath in points {

                let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
                let message = read_df_message::<NumassMeta>(&mut point_file)
                    .await
                    .unwrap();
                let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
                let monitor_coeff: f64 = if correct_to_monitor {
                    coeffs.get_from_meta(&filepath, &message.meta) as f64
                } else { 1.0 };

                let amps = post_process(
                    extract_events(&point, &PROCESSING) , &POST_PROCESSING);
                
                amps.iter().for_each(|(_, frames): (&u64, &std::collections::BTreeMap<usize, (u16, f32)>)| {

                    if check_neigbors_fast(frames) {
                        counts += monitor_coeff;
                    } else {
                        counts += frames.len() as f64 * monitor_coeff;
                    }
                });

                time += 30.0;
            }

            table.lock().await.insert(u_sp_v, counts / time);
            pb.lock().await.inc(1)       
        })
    }).collect::<Vec<_>>();
    

    for handle in handles {
        handle.await.unwrap();
    }

    let mut table_data = "".to_string();
    table.try_lock().unwrap().clone().iter().for_each(|(u_sp, count_rate)| {
        table_data.push_str(&format!(
            "{u_sp}\t{count_rate}\t{err}\n", err = count_rate * 0.1 + 100.0
        ));
    });
    std::fs::write(bgr_dir.join("table.tsv"), table_data).unwrap()
}
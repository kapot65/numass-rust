use std::{path::PathBuf, collections::BTreeMap, vec, sync::Arc, ops::Range};

use analysis::{get_points_by_pattern, workspace::{get_db_fast_root, get_hist_range, get_hist_bins, get_workspace}, amps::get_amps, ethalon::get_ethalon};
use indicatif::ProgressStyle;
use processing::{Algorithm, PostProcessParams, histogram::HistogramParams, post_process, ProcessParams, events_to_histogram};
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

const E_MIN: f32 = 4.0;
const E_MAX: f32 = 40.0;

const E_PEAK: f32 = 19.0;

const L_COEFF: f32 = 0.85;


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

    let db_root = get_db_fast_root().to_str().unwrap().to_owned();
    let run = "2023_03";

    let workspace = get_workspace();

    // === Background 1 ===
    // let pattern = format!("/{run}/Background_1/set_[12]/p*");
    // let exclude = [
    //     format!("/Background_1/set_1/p43"),
    //     format!("/Background_1/set_1/p74"),
    //     format!("/Background_1/set_2/p55"),
    //     format!("/Background_1/set_2/p59"),
    //     format!("/Background_1/set_2/p66")
    // ];
    // let correct_to_monitor = false;
    // let group = "bgr-1";

    // === Background 2,3 ===
    // let pattern = format!("/{run}/Background_2/set_[12]/p*");
    // let exclude = [];
    // let correct_to_monitor = false;
    // let group = "bgr-2";

    // === Tritium 1, Bgr 1 ===
    // let pattern = format!("/{run}/Tritium_1/set_[1234567]/p*");
    let pattern = format!("/{run}/Tritium_1/set_[567]/p*");
    let exclude = [];
    // let correct_to_monitor = true;
    let group = "tritium-1(5-7)";


    // === End ===
    std::fs::create_dir_all(&workspace).unwrap();

    let bgr_dir = workspace.join(group);
    std::fs::create_dir_all(&bgr_dir).unwrap();

    // let coeffs = Arc::new(CorrectionCoeffs::load(&format!("/{db_root}/monitor.json")));
    let points = get_points_by_pattern(&db_root, &pattern, &exclude);

    let pb: indicatif::ProgressBar = indicatif::ProgressBar::new(points.len() as u64);
    pb.set_style(ProgressStyle::with_template("[{elapsed_precise}] {bar} {pos:>7}/{len:7} {msg}")
    .unwrap());
    let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::new(Mutex::new(pb));

    let table = Arc::new(Mutex::new(BTreeMap::new()));

    let handles = points.iter().map(|(u_sp, points)| {
        let u_sp_v = *u_sp;
        let u_sp_kev = u_sp_v as f32 / 1000.0;

        let points = points.clone();
        let bgr_dir = bgr_dir.clone();
        
        let table = Arc::clone(&table);
        let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::clone(&pb);

        // let coeffs = Arc::clone(&coeffs);

        tokio::spawn(async move {
    
            let mut out_point = ProducedPoint {
                u_sp: u_sp_v,
                e_curr: E_MIN.max(18.5 - u_sp_kev),
                l_curr: u_sp_kev * L_COEFF,
                k: 0.0,
                l: 0.0,
                m: 0.0,
                d: 0.0,
                d_sum: 0.0,
                origins: vec![],
                time: 0
            };

            for filepath in points {

                let point_hist = events_to_histogram(
                    post_process(get_amps(
                        &filepath, PROCESSING).await.unwrap(),
                            &POST_PROCESSING), 
                        HistogramParams { 
                            range: get_hist_range(), 
                            bins: get_hist_bins()
                    }
                );

                let eth_hist = get_ethalon(
                    format!("/{run}/Tritium_1/set_[1234]/p*(HV1={u_sp_v})"),
                    ProcessParams::default(),
                    PostProcessParams::default()
                ).await.unwrap();

                //? правильно ли считать отношение по всем каналам?
                // надо умножать на отношение
                let point_eth_ratio = {
                    let range_l = 4.0..(u_sp_kev - 2.0);
                    let range_r = (u_sp_kev + 1.0)..30.0;

                    let eth_counts = 
                        eth_hist.events_all(Some(range_l.clone())) +
                        eth_hist.events_all(Some(range_r.clone()))
                    ;
                    let point_counts = 
                        point_hist.events_all(Some(range_l.clone())) +
                        point_hist.events_all(Some(range_r.clone()))
                    ;
                    point_counts as f64 / eth_counts as f64
                };

                // TODO: handle monitor correction
                // let monitor_coeff = if correct_to_monitor {
                //     coeffs.get_for_point(&filepath, &point) as f64
                // } else { 1.0 };

                let extract_6 = |range: Range<f32>| {
                    let counts_in_point =  *point_hist.events(Some(range.clone()))
                        .get(&5).unwrap() as f64;
                    let counts_in_eth = *eth_hist.events(Some(range))
                        .get(&5).unwrap() as f64;
                    counts_in_point - counts_in_eth * point_eth_ratio
                };

                let extract = |range: Range<f32>| {
                    let counts_in_point =  point_hist.events_all(Some(range.clone())) as f64;
                    let counts_in_eth = eth_hist.events_all(Some(range)) as f64;
                    counts_in_point - counts_in_eth * point_eth_ratio
                };

                out_point.k += extract_6(out_point.e_curr..E_PEAK);

                let l = extract_6(out_point.l_curr..E_PEAK);

                out_point.l += l;
                out_point.m += extract(out_point.l_curr..E_PEAK) - l;

                out_point.d += extract_6(E_PEAK..E_MAX);
                out_point.d_sum += extract(E_PEAK..E_MAX);

                out_point.time += 30;
                out_point.origins.push(filepath);
            }

            tokio::fs::write(
                bgr_dir.join(format!("{}.json", u_sp_v)),
                serde_json::to_string(&out_point).unwrap()).await.unwrap();

            table.lock().await.insert(u_sp_v, out_point);
            pb.lock().await.inc(1)       
        })
    }).collect::<Vec<_>>();
    

    for handle in handles {
        handle.await.unwrap();
    }

    let mut table_data = "u_sp\te_curr\tl_curr\tk\tl\tm\td\td_sum\ttime\n".to_string();
    table.try_lock().unwrap().clone().iter().for_each(|(u_sp, point)| {
        table_data.push_str(&format!(
            "{u_sp}\t{e_curr}\t{l_curr}\t{k}\t{l}\t{m}\t{d}\t{d_sum}\t{time}\n",
            u_sp = u_sp,
            e_curr = point.e_curr,
            l_curr = point.l_curr,
            k = point.k.round() as i64,
            l = point.l.round() as i64,
            m = point.m.round() as i64,
            d = point.d.round() as i64,
            d_sum = point.d_sum.round() as i64,
            time = point.time
        ).replace('.', ","));
    });
    std::fs::write(bgr_dir.join("all.tsv"), table_data).unwrap()
}
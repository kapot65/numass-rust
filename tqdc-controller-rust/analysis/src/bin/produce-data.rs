use std::{path::PathBuf, collections::BTreeMap, vec, sync::Arc};

use analysis::{get_points_by_pattern, CorrectionCoeffs};
// use analysis::DB_ROOT;
use dataforge::read_df_message;
use indicatif::ProgressStyle;
use processing::{Algorithm, numass::{NumassMeta, protos::rsb_event}, PostProcessingParams, histogram::{HistogramParams, PointHistogram}, post_process, extract_amplitudes};
use protobuf::Message;
use serde::{Serialize, Deserialize};
use tokio::sync::Mutex;

const ALGORITHM: Algorithm = Algorithm::FirstPeak { threshold: 15, left: 8 };

const POST_PROCESSING: PostProcessingParams = PostProcessingParams {
    // TODO: add to KeV corrections
    convert_to_kev: true,
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

const HISTOGRAM_PARAMS: HistogramParams = HistogramParams { range: E_MIN..E_MAX, bins: 360 };


#[derive(Debug, Serialize, Deserialize, Clone)]
struct ProducedPoint {
    u_sp: u16,
    e_curr: f32,
    l_curr: f32,
    k: f64,
    l: f64,
    m: f64,
    d: f64,
    origins: Vec<PathBuf>,
    time: u64
}

#[tokio::main]
async fn main() {

    // let db_root = "/data/numass-server";
    let db_root = "/data-ssd";
    let run = "2023_03";

    // === Background 1 ===
    let pattern = format!("/{run}/Background_1/set_[12]/p*");
    let exclude = [
        format!("/Background_1/set_1/p43"),
        format!("/Background_1/set_1/p74"),
        format!("/Background_1/set_2/p55"),
        format!("/Background_1/set_2/p59"),
        format!("/Background_1/set_2/p66")
    ];
    let correct_to_monitor = false;
    let group = "bgr-1";

    // === Background 2,3 ===
    // let pattern = format!("/{run}/Background_2/set_[12]/p*");
    // let exclude = [];
    // let correct_to_monitor = false;
    // let group = "bgr-2";

    // === Tritium 1, Bgr 1 ===
    // let pattern = format!("/{run}/Tritium_1/set_[1234567]/p*");
    // let exclude = [];
    // let correct_to_monitor = true;
    // let group = "tritium-1-bgr-1";

    // === Tritium 1, Bgr 2 ===
    // let pattern = format!("/{run}/Tritium_1/set_[123][0123456789]/p*");
    // let exclude = [
    //     "Tritium_1/set_10".to_owned()
    // ];
    // let correct_to_monitor = true;
    // let group = "tritium-1-bgr-2";

    // === Tritium 2 ===
    // let pattern = format!("/{run}/Tritium_2/set_*/p*");
    // let exclude: Vec<String> = vec![
    //     "Tritium_2/set_5/p18".to_owned(),
    //     "Tritium_2/set_5/p19".to_owned(),
    //     "Tritium_2/set_14/p20".to_owned(),
    //     "Tritium_2/set_14/p22".to_owned(),
    // ];
    // let correct_to_monitor = true;
    // let group = "tritium-2";

    // === Tritium 3 ===
    // let pattern = format!("/{run}/Tritium_3/set_*/p*");
    // let exclude: Vec<String> = vec![
    //     "Tritium_3/set_25_short".to_owned(),
    //     "Tritium_2/set_29/p37".to_owned()
    // ];
    // let correct_to_monitor = true;
    // let group = "tritium-3";

    // === Tritium 4 ===
    // let pattern = format!("/{run}/Tritium_4/set_*/p*");
    // let exclude: Vec<String> = vec![
    //     "Tritium_4/set_10_short".to_owned(),
    //     "Tritium_4/set_25/p7".to_owned(),
    //     "Tritium_4/set_25_18000V_bad".to_owned()
    // ];
    // let correct_to_monitor = true;
    // let group = "tritium-4";

    // === Tritium 5 ===
    // let pattern = format!("/{run}/Tritium_5/set_*/p*");
    // let exclude: Vec<String> = vec![
    //     "Tritium_5/set_10".to_owned()
    // ];
    // let correct_to_monitor = true;
    // let group = "tritium-5";

    // === End ===

    let workspace = PathBuf::from("/home/chernov/produced");
    std::fs::create_dir_all(&workspace).unwrap();

    let bgr_dir = workspace.join(group);
    std::fs::create_dir_all(&bgr_dir).unwrap();

    let coeffs = Arc::new(CorrectionCoeffs::load(&format!("/{db_root}/monitor.json")));
    let points = get_points_by_pattern(db_root, &pattern, &exclude);

    let pb = indicatif::ProgressBar::new(points.len() as u64);
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
        let coeffs = Arc::clone(&coeffs);

        tokio::spawn(async move {

            let mut hist = PointHistogram::new(HISTOGRAM_PARAMS.range, HISTOGRAM_PARAMS.bins);
    
            let mut out_point = ProducedPoint {
                u_sp: u_sp_v,
                e_curr: E_MIN.max(18.5 - u_sp_kev),
                l_curr: u_sp_kev as f32 * L_COEFF,
                k: 0.0,
                l: 0.0,
                m: 0.0,
                d: 0.0,
                origins: vec![],
                time: 0
            };

            for filepath in points {

                let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
                let message = read_df_message::<NumassMeta>(&mut point_file)
                    .await
                    .unwrap();
                let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
                let monitor_coeff = if correct_to_monitor {
                    coeffs.get_for_point(&filepath, &point) as f64
                } else { 1.0 };

                let amps = post_process(
                    extract_amplitudes(&point, &ALGORITHM, true) , &POST_PROCESSING);
                
                amps.iter().for_each(|(_, frames): (&u64, &std::collections::BTreeMap<usize, f32>)| {
                    frames.iter().for_each(|(ch_num, amp)| {

                        hist.add(*ch_num as u8, *amp);

                        if (out_point.e_curr..E_PEAK).contains(amp) {
                            if *ch_num == 5 {
                                out_point.k += monitor_coeff;
                                if (out_point.l_curr..E_PEAK).contains(amp) {
                                    out_point.l += monitor_coeff;
                                }
                            } else if (out_point.l_curr..E_PEAK).contains(amp) {
                                out_point.m += monitor_coeff;
                            }
                        } else if (E_PEAK..E_MAX).contains(amp) && *ch_num == 5 {
                            out_point.d += monitor_coeff;
                        }
                    })
                });

                out_point.time += 30; // TODO: remove hardcode
                out_point.origins.push(filepath);
            }

            tokio::fs::write(
                bgr_dir.join(format!("{}.json", u_sp_v)),
                serde_json::to_string(&out_point).unwrap()).await.unwrap();

            // TODO: move to PointHistogram trait
            let mut ascii_hist = format!("u\tcounts\n");
            {
                let(_, counts) = hist.channels.iter().find(|(ch_id, _)| **ch_id == 5).unwrap();
                counts.iter().enumerate().for_each(|(i, val)| {
                    ascii_hist.push_str(&format!("{}\t{}\n", hist.x[i], val));
                })
            }

            tokio::fs::write(
                bgr_dir.join(format!("{}.hist.tsv", u_sp_v)),
                ascii_hist
            ).await.unwrap();

            table.lock().await.insert(u_sp_v, out_point);
            pb.lock().await.inc(1)       
        })
    }).collect::<Vec<_>>();
    

    for handle in handles {
        handle.await.unwrap();
    }

    let mut table_data = format!("u_sp\te_curr\tl_curr\tk\tl\tm\td\ttime\n");
    table.try_lock().unwrap().clone().iter().for_each(|(u_sp, point)| {
        table_data.push_str(&format!(
            "{u_sp}\t{e_curr}\t{l_curr}\t{k}\t{l}\t{m}\t{d}\t{time}\n",
            u_sp = u_sp,
            e_curr = point.e_curr,
            l_curr = point.l_curr,
            k = point.k.round() as u64,
            l = point.l.round() as u64,
            m = point.m.round() as u64,
            d = point.d.round() as u64,
            time = point.time
        ));
    });
    std::fs::write(bgr_dir.join("all.tsv"), table_data).unwrap()
}
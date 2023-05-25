use std::{path::PathBuf, collections::BTreeMap, vec, sync::Arc};

use analysis::get_points_by_pattern;
// use analysis::DB_ROOT;
use dataforge::read_df_message;
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
    k: usize,
    l: usize,
    m: usize,
    d: usize,
    origins: Vec<PathBuf>,
    time: u64
}

#[tokio::main]
async fn main() {

    let db_root = "/home/chernov/data";

    let run = "2023_03";

    let pattern = format!("/{run}/Background_[23]/set_[123]/p*");
    let exclude = [
        format!("/Background_3/set_1/p101")
    ];
    let group = "bgr";

    // let pattern = format!("/{run}/Tritium_5/set_*/p*");
    // let exclude: Vec<String> = vec![
    //     "Tritium_5/set_10".to_owned()
    // ];
    // let group = "tritium_5";


    let workspace = PathBuf::from("/home/chernov/produced");
    std::fs::create_dir_all(&workspace).unwrap();

    let bgr_dir = workspace.join(group);
    std::fs::create_dir_all(&bgr_dir).unwrap();

    let points = get_points_by_pattern(db_root, &pattern, &exclude);

    let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::new(Mutex::new(indicatif::ProgressBar::new(points.len() as u64)));
    let table = Arc::new(Mutex::new(BTreeMap::new()));

    let handles = points.iter().map(|(u_sp, points)| {
        let u_sp_v = *u_sp;
        let u_sp_kev = u_sp_v as f32 / 1000.0;
        let points = points.clone();
        let bgr_dir = bgr_dir.clone();
        let table = Arc::clone(&table);
        let pb = Arc::clone(&pb);

        tokio::spawn(async move {

            let mut hist = PointHistogram::new(HISTOGRAM_PARAMS.range, HISTOGRAM_PARAMS.bins);
    
            let mut out_point = ProducedPoint {
                u_sp: u_sp_v,
                e_curr: E_MIN.max(18.5 - u_sp_kev),
                l_curr: u_sp_kev as f32 * L_COEFF,
                k: 0,
                l: 0,
                m: 0,
                d: 0,
                origins: vec![],
                time: 0
            };

            for filepath in points {

                let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
                let message = read_df_message::<NumassMeta>(&mut point_file)
                    .await
                    .unwrap();
                let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

                let amps = post_process(
                    extract_amplitudes(&point, &ALGORITHM, true) , &POST_PROCESSING);
                
                amps.iter().for_each(|(_, frames): (&u64, &std::collections::BTreeMap<usize, f32>)| {
                    frames.iter().for_each(|(ch_num, amp)| {

                        hist.add(*ch_num as u8, *amp);

                        if (out_point.e_curr..E_PEAK).contains(amp) {
                            if *ch_num == 5 {
                                out_point.k += 1;
                                if (out_point.l_curr..E_PEAK).contains(amp) {
                                    out_point.l += 1;
                                }
                            } else if (out_point.l_curr..E_PEAK).contains(amp) {
                                out_point.m += 1;
                            }
                        } else if (E_PEAK..E_MAX).contains(amp) && *ch_num == 5 {
                            out_point.d += 1;
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
            k = point.k,
            l = point.l,
            m = point.m,
            d = point.d,
            time = point.time
        ));
    });
    std::fs::write(bgr_dir.join("all.tsv"), table_data).unwrap()
}
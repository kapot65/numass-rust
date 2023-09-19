use std::{path::PathBuf, collections::BTreeMap, vec, sync::Arc};

use analysis::{get_points_by_pattern, CorrectionCoeffs};
use dataforge::read_df_message;
use indicatif::ProgressStyle;
use processing::{Algorithm, numass::{NumassMeta, protos::rsb_event}, PostProcessParams, histogram::{HistogramParams, PointHistogram}, post_process, extract_amplitudes, ProcessParams};
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

const E_MIN: f32 = 4.0;
const E_MAX: f32 = 40.0;

const E_PEAK: f32 = 19.0;

const HISTOGRAM_PARAMS: HistogramParams = HistogramParams { range: E_MIN..E_MAX, bins: 360 };


#[derive(Debug, Serialize, Deserialize, Clone)]
struct ProducedPoint {
    u_sp: u16,
    e_curr: f32,

    // ch 6
    a: f64, /// [min..U_sp-2]
    b: f64, /// [U_sp-2..U_sp+1]
    c: f64, /// [U_sp+1..19]
    d: f64, /// [19..40]

    // other channels
    a_oth: f64, /// [min..U_sp-2]
    b_oth: f64, /// [U_sp-2..U_sp+1]
    c_oth: f64, /// [U_sp+1..19]
    d_oth: f64, /// [19..40]

    origins: Vec<PathBuf>,
    time: u64
}

#[tokio::main]
async fn main() {

    let db_root = "/data-nvme";
    let run = "2023_03";

    let workspace = PathBuf::from("/home/chernov/produced/patrial-abcd");

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
    // let pattern = format!("/{run}/Tritium_1/set_[567]/p*");
    // let exclude = [];
    // let correct_to_monitor = true;
    // let group = "tritium-1-bgr-1(5-7)";

    // === Tritium 1, Bgr 2 ===
    let pattern = format!("/{run}/Tritium_1/set_[12]/p*");
    let exclude = [
        "Tritium_1/set_10".to_owned()
    ];
    let correct_to_monitor = true;
    let group = "tritium-1-bgr-2(1,2)";

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
    //     "Tritium_3/set_1/".to_owned(),
    //     "Tritium_3/set_2/".to_owned(),
    //     "Tritium_3/set_3/".to_owned(),
    //     "Tritium_3/set_4/".to_owned(),

    //     "Tritium_3/set_5/".to_owned(),
    //     "Tritium_3/set_6/".to_owned(),
    //     "Tritium_3/set_7/".to_owned(),
    //     "Tritium_3/set_8/".to_owned(),

    //     "Tritium_3/set_9/".to_owned(),
    //     "Tritium_3/set_10/".to_owned(),
    //     "Tritium_3/set_11/".to_owned(),
    //     "Tritium_3/set_12/".to_owned(),

    //     "Tritium_3/set_13/".to_owned(),
    //     "Tritium_3/set_14/".to_owned(),
    //     "Tritium_3/set_15/".to_owned(),
    //     "Tritium_3/set_16/".to_owned(),

    //     "Tritium_3/set_17/".to_owned(),
    //     "Tritium_3/set_18/".to_owned(),
    //     "Tritium_3/set_19/".to_owned(),
    //     "Tritium_3/set_20/".to_owned(),

    //     "Tritium_3/set_21/".to_owned(),
    //     "Tritium_3/set_22/".to_owned(),
    //     "Tritium_3/set_23/".to_owned(),
    //     "Tritium_3/set_24/".to_owned(),

    //     "Tritium_3/set_25/".to_owned(),
    //     "Tritium_3/set_26/".to_owned(),
    //     "Tritium_3/set_27/".to_owned(),
    //     "Tritium_3/set_28/".to_owned(),

    //     // "Tritium_3/set_29/".to_owned(),
    //     // "Tritium_3/set_30/".to_owned(),
    //     // "Tritium_3/set_31/".to_owned(),

    //     "Tritium_3/set_25_short".to_owned(),
    //     "Tritium_2/set_29/p37".to_owned()
    // ];
    // let correct_to_monitor = true;
    // let group = "tritium-3-(29-31)";

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
    //     "Tritium_5/set_1/".to_owned(),
    //     "Tritium_5/set_2/".to_owned(),
    //     "Tritium_5/set_3/".to_owned(),
    //     "Tritium_5/set_4/".to_owned(),

    //     "Tritium_5/set_5/".to_owned(),
    //     "Tritium_5/set_6/".to_owned(),
    //     "Tritium_5/set_7/".to_owned(),
    //     "Tritium_5/set_8/".to_owned(),

    //     // "Tritium_5/set_9/".to_owned(),
    //     // "Tritium_5/set_10/".to_owned(),
    //     // "Tritium_5/set_11/".to_owned(),
    //     // "Tritium_5/set_12/".to_owned(),

    //     "Tritium_5/set_10".to_owned()
    // ];

    // let correct_to_monitor = true;
    // let group = "tritium-5(9,11,12)";

    // === End ===
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
                // l_curr: u_sp_kev * L_COEFF,
                a: 0.0,
                b: 0.0,
                c: 0.0,
                d: 0.0,

                a_oth: 0.0,
                b_oth: 0.0,
                c_oth: 0.0,
                d_oth: 0.0,

                // d_sum: 0.0,
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
                    extract_amplitudes(&point, &PROCESSING) , &POST_PROCESSING);
                
                amps.iter().for_each(|(_, frames): (&u64, &std::collections::BTreeMap<usize, f32>)| {
                    frames.iter().for_each(|(ch_num, amp)| {
                        hist.add(*ch_num as u8, *amp);
                        if *ch_num == 5 {
                            if (out_point.e_curr..u_sp_kev-2.0).contains(amp) {
                                out_point.a += monitor_coeff;
                            } else if (u_sp_kev-2.0..u_sp_kev+1.0).contains(amp) {
                                out_point.b += monitor_coeff;
                            } else if u_sp_kev < 18.0 && (u_sp_kev+1.0..E_PEAK).contains(amp) {
                                out_point.c += monitor_coeff;
                            } else if  (E_PEAK..E_MAX).contains(amp) {
                                out_point.d += monitor_coeff;
                            }
                        } else if (out_point.e_curr..u_sp_kev-2.0).contains(amp) {
                            out_point.a_oth += monitor_coeff;
                        } else if (u_sp_kev-2.0..u_sp_kev+1.0).contains(amp) {
                            out_point.b_oth += monitor_coeff;
                        } else if u_sp_kev < 18.0 && (u_sp_kev+1.0..E_PEAK).contains(amp) {
                            out_point.c_oth += monitor_coeff;
                        } else if  (E_PEAK..E_MAX).contains(amp) {
                            out_point.d_oth += monitor_coeff;
                        };
                    })
                });

                out_point.time += 30; // TODO: remove hardcode
                out_point.origins.push(filepath);
            }

            tokio::fs::write(
                bgr_dir.join(format!("{}.json", u_sp_v)),
                serde_json::to_string(&out_point).unwrap()).await.unwrap();

            // TODO: move to PointHistogram trait
            let mut ascii_hist = "u\tcounts\n".to_string();
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

    let mut table_data = "u_sp\te_curr\ta\tb\tc\td\ta_oth\tb_oth\tc_oth\td_oth\ttime\n".to_string();
    table.try_lock().unwrap().clone().iter().for_each(|(u_sp, point)| {
        table_data.push_str(&format!(
            "{u_sp}\t{e_curr}\t{a}\t{b}\t{c}\t{d}\t{a_oth}\t{b_oth}\t{c_oth}\t{d_oth}\t{time}\n",
            u_sp = u_sp,
            e_curr = point.e_curr,
            a = point.a.round() as u64,
            b = point.b.round() as u64,
            c = point.c.round() as u64,
            d = point.d.round() as u64,
            a_oth = point.a_oth.round() as u64,
            b_oth = point.b_oth.round() as u64,
            c_oth = point.c_oth.round() as u64,
            d_oth = point.d_oth.round() as u64,
            time = point.time
        ).replace('.', ","));
    });
    std::fs::write(bgr_dir.join("all.tsv"), table_data).unwrap()
}
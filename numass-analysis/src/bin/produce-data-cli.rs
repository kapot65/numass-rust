//! Основная обработка данных сеанса
//! 
//! Скрипт:
//! - объединяет точки в группы по HV (с учетом мониторинга)
//! - каждую группу переводит в формат [ProducedPoint]
//! - сохраняет все получившиеся [ProducedPoint] в tsv таблицу
//! 
//! На вход принимается yaml файл со структурой [Opts]
//! 
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, vec};

use clap::Parser;
use indicatif::ProgressStyle;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use analysis::CorrectionCoeffs;
use processing::{
    postprocess::{post_process, PostProcessParams},
    process::ProcessParams,
    storage::process_point,
    types::FrameEvent,
};

#[cfg(target_family = "unix")]
use tikv_jemallocator::Jemalloc;
#[cfg(target_family = "unix")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ProducedPoint {
    u_sp: u16,
    l_curr: f32,
    k: f64,        // 5..19.5 keV
    l: f64,        // 5..[(U_sp -11000)/1000 +2] keV
    doubles: f64,  // 19.5..33.5 keV (двойные)
    tripples: f64, // 33.5..50 keV (тройные)
    triggers: usize,
    bad: usize,
    time: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Set {
    exclude: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Opts {
    db_root: PathBuf,
    run: String,
    groups: BTreeMap<String, BTreeMap<String, Set>>,
    processing: ProcessParams,
    post_processing: PostProcessParams,
    monitor: Option<PathBuf>,
}

#[derive(Parser, Debug, Clone)]
#[clap(author, version, about, long_about = None)]
pub struct Arg {
    /// Path to the config file in yaml format
    pub config_file: PathBuf,
}

#[tokio::main] // TODO adjust worker_threads ( #[tokio::main(worker_threads = ?)] )
async fn main() {
    let args = Arg::parse();
    let opts: Opts =
        serde_yaml::from_reader(std::fs::File::open(&args.config_file).unwrap()).unwrap();

    let mut points = BTreeMap::new();

    opts.groups.iter().for_each(|(group_name, group)| {
        group.iter().for_each(|(set_num, set)| {
            let set_directory = opts
                .db_root
                .join(opts.run.clone())
                .join(group_name.clone())
                .join(format!("set_{}", set_num));

            let files = std::fs::read_dir(set_directory)
                .unwrap()
                .filter_map(|file| {
                    if let Ok(file) = file {
                        let file_name = file.file_name().into_string().unwrap();
                        if file_name.starts_with('p') {
                            if let Some(exclude) = &set.exclude {
                                if exclude.iter().any(|ex| file_name.contains(ex)) {
                                    None
                                } else {
                                    Some(file.path())
                                }
                            } else {
                                Some(file.path())
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });

            for point in files {
                let point_name = point.file_name().unwrap().to_str().unwrap().to_owned();

                if let Some(exclude) = &set.exclude {
                    if exclude.iter().any(|ex| point_name.contains(ex)) {
                        continue;
                    }
                }

                let u_sp = {
                    let point_name = point_name.as_str();
                    point_name[point_name.len() - 6..point_name.len() - 1]
                        .parse::<u16>()
                        .unwrap()
                };

                points.entry(u_sp).or_insert(vec![]).push(point);
            }
        })
    });

    let coeffs = Arc::new(
        opts.monitor
            .map(|filepath| CorrectionCoeffs::load(filepath.to_str().unwrap())),
    );

    let pb = indicatif::ProgressBar::new(points.len() as u64);
    pb.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] {bar} {pos:>7}/{len:7} {msg}").unwrap(),
    );
    let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::new(Mutex::new(pb));

    let table = Arc::new(Mutex::new(BTreeMap::new()));

    let handles = points
        .iter()
        .map(|(u_sp, points)| {
            let u_sp_v = *u_sp;
            let u_sp_kev = u_sp_v as f32 / 1000.0;
            let points = points.clone();
            let table = Arc::clone(&table);
            let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::clone(&pb);
            let coeffs = Arc::clone(&coeffs);

            let processing = opts.processing.clone();
            let post_processing = opts.post_processing;

            tokio::spawn(async move {
                // let mut hist = PointHistogram::new(hist_params.range, hist_params.bins);

                let mut out_point = ProducedPoint {
                    u_sp: u_sp_v,
                    l_curr: u_sp_kev - 11.0 + 6.0,
                    k: 0.0,
                    l: 0.0,
                    doubles: 0.0,
                    tripples: 0.0,
                    triggers: 0,
                    bad: 0,
                    time: 0,
                };

                for filepath in points {
                    let monitor_coeff = if let Some(coeffs) = coeffs.as_ref() {
                        coeffs.get_by_index(&filepath) as f64
                    } else {
                        1.0
                    };

                    let (frames, preprocess) = process_point(&filepath, &processing)
                        .await
                        .unwrap()
                        .1
                        .unwrap();

                    out_point.triggers += frames.len();

                    let (frames, _) = post_process((frames, preprocess), &post_processing);

                    frames.iter().for_each(|(_, events)| {                        
                        let mut is_bad = false;

                        if events.is_empty() {
                            is_bad = true;
                        } else {
                            events.iter().for_each(|(_, event)| match event {
                                FrameEvent::Event { amplitude, .. } => {
                                    if (4.0..19.5).contains(amplitude) {
                                        out_point.k += monitor_coeff;
                                        if (4.0..out_point.l_curr).contains(amplitude) {
                                            out_point.l += monitor_coeff;
                                        }
                                    } else if (19.5..33.5).contains(amplitude) {
                                        out_point.doubles += monitor_coeff;
                                    } else if (33.5..50.0).contains(amplitude) {
                                        out_point.tripples += monitor_coeff;
                                    }
                                }
                                FrameEvent::Reset { .. } => {
                                    is_bad = true;
                                }
                                FrameEvent::Overflow { .. } => {
                                    is_bad = true;
                                }
                                FrameEvent::Frame { .. } => {}
                            })
                        }

                        if is_bad {
                            out_point.bad += 1;
                        }
                    });

                    out_point.time += 30; // TODO: remove hardcode
                                          // out_point.origins.push(filepath);
                }

                table.lock().await.insert(u_sp_v, out_point);
                pb.lock().await.inc(1)
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.await.unwrap();
    }

    let mut table_data = "u_sp\tl_curr\tk\tl\tdoubles\ttripples\tframes\tbad\ttime\n".to_string();
    table
        .try_lock()
        .unwrap()
        .clone()
        .iter()
        .for_each(|(u_sp, point)| {
            table_data.push_str(&format!(
                "{u_sp}\t{l_curr}\t{k}\t{l}\t{d}\t{t}\t{f}\t{r}\t{time}\n",
                l_curr = point.l_curr,
                k = point.k.round() as u64,
                l = point.l.round() as u64,
                d = point.doubles.round() as u64,
                t = point.tripples.round() as u64,
                f = point.triggers,
                r = point.bad,
                time = point.time
            ));
        });

    std::fs::write(args.config_file.with_extension("tsv"), table_data).unwrap()
}

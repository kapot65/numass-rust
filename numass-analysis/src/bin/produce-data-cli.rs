//! Основная обработка данных сеанса
//!
//! Скрипт:
//! - объединяет точки в группы по HV (с учетом мониторинга)
//! - каждую группу переводит в формат [ProducedPoint]
//! - сохраняет все получившиеся [ProducedPoint] в tsv таблицу
//!
//! На вход принимается yaml файл со структурой [Opts]
//!
use std::{collections::BTreeMap, ops::Range, path::PathBuf, sync::Arc, vec};

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

enum RANGES {
    Singles,
    Doubles,
    Tripples,
    Quadriples,
}

impl RANGES {
    fn values(&self) -> Range<f32> {
        match self {
            RANGES::Singles => 4.5..18.5,
            RANGES::Doubles => 18.5..31.0,
            RANGES::Tripples => 31.0..45.0,
            RANGES::Quadriples => 45.0..60.0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ProducedPoint {
    u_sp: u16,
    singles: f64,
    doubles: f64,
    tripples: f64,
    quadriples: f64,
    triggers: usize,
    bad: usize,
    time: f64,
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

            let files = std::fs::read_dir(&set_directory)
                .expect(&format!("Failed to read directory: {:?}", set_directory))
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
            // let u_sp_kev = u_sp_v as f32 / 1000.0;
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
                    singles: 0.0,
                    doubles: 0.0,
                    tripples: 0.0,
                    quadriples: 0.0,
                    triggers: 0,
                    bad: 0,
                    time: 0.0,
                };

                for filepath in points {
                    let monitor_coeff = if let Some(coeffs) = coeffs.as_ref() {
                        coeffs.get_by_index(&filepath) as f64
                    } else {
                        1.0 // Default to 1.0 if no coefficients are provided (needed for Background)
                    };

                    let (frames, preprocess) = process_point(&filepath, &processing)
                        .await
                        .unwrap()
                        .1
                        .unwrap();

                    let (frames, preprocess) = post_process((frames, preprocess), &post_processing);
                    out_point.triggers += frames.len();

                    frames.iter().for_each(|(_, events)| {
                        let mut is_bad = false;

                        if events.is_empty() {
                            is_bad = true;
                        } else {
                            events.iter().for_each(|(_, event)| match event {
                                FrameEvent::Event { amplitude, .. } => {
                                    if (RANGES::Singles.values()).contains(amplitude) {
                                        out_point.singles += monitor_coeff;
                                    } else if (RANGES::Doubles.values()).contains(amplitude) {
                                        out_point.doubles += monitor_coeff;
                                    } else if (RANGES::Tripples.values()).contains(amplitude) {
                                        out_point.tripples += monitor_coeff;
                                    } else if (RANGES::Quadriples.values()).contains(amplitude) {
                                        out_point.quadriples += monitor_coeff;
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

                    if post_processing.merge_close_events && post_processing.cut_bad_blocks {
                        out_point.time += (preprocess.effective_time() / 1_000_000_000) as f64;
                    } else {
                        out_point.time += 30.0; // TODO: remove hardcode
                    }
                }

                table.lock().await.insert(u_sp_v, out_point);
                pb.lock().await.inc(1)
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.await.unwrap();
    }

    let mut table_data =
        format!(
            "u_sp\tsingles({:?})\tdoubles({:?})\ttripples({:?})\tquadriples({:?})\ttriggers\tbad\ttime\n",
            RANGES::Singles.values(),
            RANGES::Doubles.values(),
            RANGES::Tripples.values(),
            RANGES::Quadriples.values()
        );
    table
        .try_lock()
        .unwrap()
        .clone()
        .iter()
        .for_each(|(u_sp, point)| {
            table_data.push_str(&format!(
                "{u_sp}\t{singles}\t{doubles}\t{tripples}\t{quadriples}\t{triggers}\t{bad}\t{time}\n",
                singles = point.singles.round() as u64,
                doubles = point.doubles.round() as u64,
                tripples = point.tripples.round() as u64,
                quadriples = point.quadriples.round() as u64,
                triggers = point.triggers,
                bad = point.bad,
                time = point.time
            ));
        });

    std::fs::write(args.config_file.with_extension("tsv"), table_data).unwrap()
}

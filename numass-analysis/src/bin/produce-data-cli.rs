use std::{path::PathBuf, collections::BTreeMap, vec, sync::Arc};

use clap::Parser;
use indicatif::ProgressStyle;
use serde::{Serialize, Deserialize};
use tokio::sync::Mutex;

use analysis::CorrectionCoeffs;
use processing::{
    postprocess::{post_process, PostProcessParams}, 
    process::ProcessParams, 
    storage::{load_meta, process_point}
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

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Set {
    exclude: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Opts {
    db_root: PathBuf,
    run: String,
    groups: BTreeMap<String, BTreeMap<usize, Set>>,
    e_min: f32,
    e_max: f32,
    e_peak: f32,
    l_coeff: f32,
    processing: ProcessParams,
    post_processing: PostProcessParams,
    monitor: Option<PathBuf>
}

#[derive(Parser, Debug, Clone)]
#[clap(author, version, about, long_about = None)]
pub struct Arg {
    /// Path to the config file in yaml format
    pub config_file: PathBuf
}

#[tokio::main]
async fn main() {

    let args = Arg::parse();
    let opts: Opts = serde_yaml::from_reader(
            std::fs::File::open(&args.config_file).unwrap()).unwrap();

    let mut points = BTreeMap::new();

    opts.groups.iter().for_each(|(group_name, group)| {
        group.iter().for_each(|(set_num, set)| {
            let set_directory = opts.db_root.join(opts.run.clone()).join(group_name.clone()).join(format!("set_{}", set_num));

            let files = std::fs::read_dir(set_directory).unwrap().filter_map(|file| {
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
                    point_name[point_name.len() - 6..point_name.len() - 1].parse::<u16>().unwrap()
                };

                points.entry(u_sp).or_insert(vec![]).push(point);
            }
        })
    });

    // let hist_params: HistogramParams = HistogramParams { range: opts.e_min..opts.e_max, bins: 360 };
    
    let coeffs = Arc::new(
        opts.monitor.map(|filepath| {
            CorrectionCoeffs::load(filepath.to_str().unwrap())
        })
    );

    let pb = indicatif::ProgressBar::new(points.len() as u64);
    pb.set_style(ProgressStyle::with_template("[{elapsed_precise}] {bar} {pos:>7}/{len:7} {msg}")
    .unwrap());
    let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::new(Mutex::new(pb));

    let table = Arc::new(Mutex::new(BTreeMap::new()));

    let handles = points.iter().map(|(u_sp, points)| {
        let u_sp_v = *u_sp;
        let u_sp_kev = u_sp_v as f32 / 1000.0;
        let points = points.clone();
        let table = Arc::clone(&table);
        let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::clone(&pb);
        let coeffs = Arc::clone(&coeffs);

        // let hist_params = hist_params.clone();
        let e_min = opts.e_min;
        let e_max = opts.e_max;
        let e_peak = opts.e_peak;
        let l_coeff = opts.l_coeff;

        let processing = opts.processing.clone();
        let post_processing = opts.post_processing;

        tokio::spawn(async move {

            // let mut hist = PointHistogram::new(hist_params.range, hist_params.bins);
    
            let mut out_point = ProducedPoint {
                u_sp: u_sp_v,
                e_curr: e_min.max(18.5 - u_sp_kev),
                l_curr: u_sp_kev * l_coeff,
                k: 0.0,
                l: 0.0,
                m: 0.0,
                d: 0.0,
                d_sum: 0.0,
                origins: vec![],
                time: 0
            };

            for filepath in points {

                let meta = load_meta(&filepath).await.unwrap();
                let monitor_coeff = if let Some(coeffs) = coeffs.as_ref() {
                    coeffs.get_from_meta(&filepath, &meta) as f64
                } else { 1.0 };

                let amps = post_process(
                    process_point(&filepath, &processing).await.unwrap().1.unwrap(),
                    &post_processing);
                
                amps.iter().for_each(|(_, frames)| {
                    frames.iter().for_each(|(ch_num, events)| {
                        for (_, amp) in events {
                            // hist.add(*ch_num as u8, *amp);
                            if (out_point.e_curr..e_peak).contains(amp) {
                                if *ch_num == 5 {
                                    out_point.k += monitor_coeff;
                                    if (out_point.l_curr..e_peak).contains(amp) {
                                        out_point.l += monitor_coeff;
                                    }
                                } else if (out_point.l_curr..e_peak).contains(amp) {
                                    out_point.m += monitor_coeff;
                                }
                            } else if (e_peak..e_max).contains(amp)  {
                                out_point.d_sum += monitor_coeff;
                            
                                if *ch_num == 5 {
                                    out_point.d += monitor_coeff;
                                }
                            }
                        }
                    })
                });

                out_point.time += 30; // TODO: remove hardcode
                out_point.origins.push(filepath);
            }

            // TODO: move to PointHistogram trait
            // let mut ascii_hist = "u\tcounts\n".to_string();
            // {
            //     let(_, counts) = hist.channels.iter().find(|(ch_id, _)| **ch_id == 5).unwrap();
            //     counts.iter().enumerate().for_each(|(i, val)| {
            //         ascii_hist.push_str(&format!("{}\t{}\n", hist.x[i], val));
            //     })
            // }

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
            k = point.k.round() as u64,
            l = point.l.round() as u64,
            m = point.m.round() as u64,
            d = point.d.round() as u64,
            d_sum = point.d_sum.round() as u64,
            time = point.time
        ).replace('.', ","));
    });

    std::fs::write(args.config_file.with_extension("tsv"), table_data).unwrap()
}
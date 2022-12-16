use std::time::SystemTime;
use std::{path::PathBuf, collections::HashMap, sync::Arc};

use protobuf::Message;

use tokio::sync::{watch, Mutex};
use eframe::egui;
use eframe::egui::plot::{Plot, Line, Legend};
use clap::Parser;


use dataforge::Reply;
use dataforge::protos::rsb_event;
use apps::{point_to_histogramm, PointHistogramm, ProcessingParams, Algorithm};


#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Opt {
    #[clap(long)]
    directory: Option<PathBuf>,
}

#[derive(Debug)]
struct FileCache {
    opened: bool,
    processed: Option<SystemTime>,
    histogram: Option<PointHistogramm>
}

struct MyApp {
    processing_params: Arc<Mutex<ProcessingParams>>,
    root: Option<FSRepr>,
    state: Arc<Mutex<HashMap<String, FileCache>>>,
    background_pipe: watch::Sender<Option<Action>>
}

#[derive(Debug, Clone, Copy)]
enum Action {
    CalculateHistogram
}

#[derive(Debug)]
enum FSRepr {
    File {
        path: PathBuf,
    },
    Directory {
        path: PathBuf,
        children: Vec<FSRepr>
    }
}

impl FSRepr {
    fn to_filename(&self) -> &str{
        let path = match self {
            FSRepr::File { path }  => path,
            FSRepr::Directory { path, children: _ } => path
        };
        path.file_name().unwrap().to_str().unwrap()
    }
}

async fn background_processing(
        mut tx: watch::Receiver<Option<Action>>, 
        configuration: Arc<Mutex<HashMap<String, FileCache>>>,
        processing_params: Arc<Mutex<ProcessingParams>>
    ) {

    let mut last_params = *processing_params.lock().await;

    while tx.changed().await.is_ok() {


        let params = *processing_params.lock().await; 

        let need_recalc = if last_params != params {
            last_params = params;
            true
        } else {
            false
        };

        let action = (*tx.borrow()).unwrap();
        match action {
            Action::CalculateHistogram => {

                let files_to_processed = {
                    let conf = configuration.lock().await;
                    conf.iter().filter_map(|(filepath, cache)| {
                        if cache.opened {
                            if need_recalc {
                                Some(filepath.clone())
                            } else if let Some(processed) = cache.processed {
                                let meta = std::fs::metadata(filepath).unwrap();
                                if processed >= meta.modified().unwrap() {
                                    None
                                } else {
                                    Some(filepath.clone())
                                }
                            } else {
                                Some(filepath.clone())
                            }
                        } else {
                            None
                        }
                    }).collect::<Vec<_>>()
                };

                for filepath in files_to_processed {

                    let configuration_local = configuration.clone(); 

                    tokio::spawn(async move {
                        let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
                        let message = dataforge::extract_df_message(&mut point_file).await.unwrap();
                        match message.meta {
                            dataforge::DFMeta::Reply(Reply::AcquirePoint { 
                                acquisition_time: _, 
                                start_time: _, 
                                end_time: _, 
                                external_meta: _, 
                                config: _, 
                                zero_suppression: _,
                                status: _
                            }) => {
                                let data = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
                                    let processed = std::fs::metadata(&filepath).unwrap().modified().unwrap();

                                    let histogram = point_to_histogramm(&data, params).await;
                                    let mut conf = configuration_local.lock().await;
                                    conf.entry(filepath).and_modify(|cache| {
                                            cache.processed = Some(processed);
                                            cache.histogram = Some(histogram)
                                        }
                                    );
                            }
                            _ => {
                                println!("{filepath:?} is not processed");
                            }
                        }
                    });
                }
            }
        }
    }
}

fn expand_dir(path: PathBuf) -> FSRepr {
    let meta = std::fs::metadata(&path).unwrap();
    if meta.is_file() {
        FSRepr::File { path }

    } else if meta.is_dir() {
        let children = std::fs::read_dir(&path).unwrap();

        let mut children = children.map(|child| {
            let entry = child.unwrap();
            expand_dir(entry.path())
        }).collect::<Vec<_>>();

        children.sort_by(|a, b| {
            natord::compare(a.to_filename(), b.to_filename())
        });

        FSRepr::Directory { path, children }
    } else {
        panic!()
    }
}

fn file_tree_entry(
    ui: &mut egui::Ui, 
    entry: &FSRepr, 
    opened_files: &mut HashMap<String, FileCache>,
) {
    match entry {
        FSRepr::File { path } => {
            let cache =  opened_files.entry(path.to_str().unwrap().to_string()).or_insert(FileCache { 
                opened: false, 
                processed: None,
                histogram: None 
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut cache.opened, "");
                let filename = path.file_name().unwrap().to_str().unwrap();
                // let status_glyph = if let Some(processed) = cache.processed {
                //     let meta = std::fs::metadata(path).unwrap();
                //     if meta.modified().unwrap() > processed {
                //         "+"
                //     } else {
                //         "."
                //     }                    
                // } else {
                //     "🕹️"
                // };
                // 
                // ui.label(format!("{filename} {status_glyph}"));
                ui.label(filename);
            });
        }

        FSRepr::Directory { path, children } => {
            egui::CollapsingHeader::new(path.file_name().unwrap().to_str().unwrap())
            .id_source(path.to_str().unwrap())
            .show(ui, |ui| {
                for child in children {
                    file_tree_entry(ui, child, opened_files) 
                }
            });
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        egui::SidePanel::left("left").show(ctx, |ui| {

            if let Ok(mut processing_params) = self.processing_params.try_lock() {

                let mut algorithm = processing_params.algorithm;

                ui.horizontal(|ui| {
                    if ui.add(egui::RadioButton::new(algorithm == Algorithm::Max, "Max")).clicked() {
                        algorithm = Algorithm::Max
                    }
                    
                    if ui.add(egui::RadioButton::new(
                        matches!(algorithm, Algorithm::Likhovid { .. }), "Likhovid")).clicked() {
                        algorithm = Algorithm::Likhovid { left: 3, right: 19 } // TODO remove hardcode
                    }
                });

                if let Algorithm::Likhovid { left, right } = algorithm {

                    ui.separator();
                    ui.label("Algorithm params");

                    let mut left = left;
                    ui.add(egui::Slider::new(&mut left, 0..=5).text("left"));
                    let mut right = right;
                    ui.add(egui::Slider::new(&mut right, 0..=40).text("right"));

                    algorithm = Algorithm::Likhovid { 
                        left, 
                        right
                    }
                }

                ui.separator();
                ui.label("Histogramm params");

                let mut hist_min = processing_params.hist_min;
                ui.add(egui::Slider::new(&mut hist_min, -10.0..=400.0).text("left"));
                let mut hist_max = processing_params.hist_max;
                ui.add(egui::Slider::new(&mut hist_max, -10.0..=400.0).text("right"));
                let mut hist_bins = processing_params.hist_bins;
                ui.add(egui::Slider::new(&mut hist_bins, 10..=2000).text("bins"));


                let mut convert_to_kev = processing_params.convert_to_kev;

                ui.checkbox(&mut convert_to_kev, "convert to keV");
                
                *processing_params = ProcessingParams {
                    algorithm,
                    convert_to_kev,
                    hist_min,
                    hist_max,
                    hist_bins,
                };
                ui.separator();
            }

            ui.horizontal(|ui| {
                if ui.button("open").clicked() {
                    if let Some(root_path) = rfd::FileDialog::new().pick_folder() {
                        self.root = Some(expand_dir(root_path))
                    }
                }
                if let Some(root) = &self.root {
                    if ui.button("reload").clicked() {
                        self.root = Some(expand_dir(match root {
                            FSRepr::File { path } => path.to_owned(),
                            FSRepr::Directory { path, children: _ } => path.to_owned()
                        }));
                    }
                }
                if ui.button("apply").clicked() {
                    self.background_pipe.send(Some(Action::CalculateHistogram)).unwrap();
                }
            });
            egui::containers::ScrollArea::new([false, true]).show(ui, |ui| {
                if let Some(root) = &mut self.root {
                    if let Ok(ref mut mutex) = self.state.try_lock() {
                        file_tree_entry(ui,root, mutex);
                    }
                }
            })
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Ok(state) = self.state.try_lock() {

                let opened_files = state.iter().filter(|(_, cache)| {
                    cache.opened
                });

                if opened_files.clone().count() == 1 {
                    let (_, hash) = opened_files.last().unwrap();
                    if hash.opened && hash.histogram.is_some() {
                        let hist = hash.histogram.clone().unwrap();

                        let lines = hist.channels.iter().map(|(ch_num, y)| {
                            Line::new(
                                y.iter().enumerate().flat_map(|(x, y)| [
                                    [(hist.x[x] - hist.step / 2.0)  as f64, *y as f64],
                                    [(hist.x[x] + hist.step / 2.0)  as f64, *y as f64]
                                ]).collect::<Vec<_>>()).name(
                                    format!("ch #{}", ch_num + 1)
                                )
                        });

                        Plot::new("Test Plot").legend(Legend { 
                            text_style: egui::TextStyle::Body, 
                            background_alpha: 1.0, position: egui::plot::Corner::RightTop 
                        })
                        .show(ui, |plot_ui| {
                            lines.for_each(|line| {
                                plot_ui.line(line);
                            })
                        });
                    }
                } else {
                    let lines = opened_files
                    .filter(|(_, cache)| {cache.histogram.is_some()})
                    .map(|(filepath, hash)| {
                        let hist = hash.histogram.clone().unwrap();

                        let mut y_all = vec![0.0; hist.x.len()];
                        for (_, y) in hist.channels {
                            for (idx, val) in y.iter().enumerate() {
                                y_all[idx] += val;
                            }
                        }

                        Line::new(
                            y_all.iter().enumerate().flat_map(|(x, y)| [
                                [(hist.x[x] - hist.step / 2.0)  as f64, *y as f64],
                                [(hist.x[x] + hist.step / 2.0)  as f64, *y as f64]
                            ]).collect::<Vec<_>>()).name(
                            filepath.to_string()
                        )
                    });

                    if lines.clone().count() > 0 {
                        Plot::new("Test Plot").legend(Legend { 
                            text_style: egui::TextStyle::Body, 
                            background_alpha: 1.0, position: egui::plot::Corner::RightTop 
                        })
                        .show(ui, |plot_ui| {
                            lines.for_each(|line| {
                                plot_ui.line(line);
                            })
                        });
                    }
                }
            }
        });
    }
}

#[tokio::main]
async fn main() {

    let opt = Opt::parse();

    let state = Arc::new(Mutex::new(HashMap::<String, FileCache>::new()));
    let configuration = Arc::clone(&state);

    let processing_params = Arc::new(Mutex::new(ProcessingParams {
        algorithm: Algorithm::Likhovid { left: 3, right: 19 },
        convert_to_kev: true,
        hist_min: 0.0,
        hist_max: 27.0,
        hist_bins: 270
    }));
    let processing_params_bg = Arc::clone(&processing_params);

    let (rx, tx) = watch::channel(None::<Action>);

    tokio::spawn(background_processing(tx, configuration, processing_params_bg));

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "data-viewer",
        options,
        Box::new(|_cc| {
            Box::new(MyApp {
                processing_params,
                root: opt.directory.map(expand_dir),
                state,
                background_pipe: rx
            })
        }),
    );
}

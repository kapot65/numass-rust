use std::time::SystemTime;
use std::{path::PathBuf, collections::HashMap, sync::Arc};

use dataforge::Reply;
use protobuf::Message;
use dataforge::protos::rsb_event;
use apps::{point_to_histogramm, PointHistogramm};
use tokio::sync::{watch, Mutex};
use eframe::egui;
use eframe::egui::plot::{Plot, Line, Legend};
use clap::Parser;

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

async fn background_processing(mut tx: watch::Receiver<Option<Action>>, configuration: Arc<Mutex<HashMap<String, FileCache>>>) {
    while tx.changed().await.is_ok() {

        let action = (*tx.borrow()).unwrap();
        match action {
            Action::CalculateHistogram => {

                let files_to_processed = {
                    let conf = configuration.lock().await;
                    conf.iter().filter_map(|(filepath, cache)| {
                        if cache.opened {
                            if let Some(processed) = cache.processed {
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
                                status: _
                            }) => {
                                let data = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
                                    let processed = std::fs::metadata(&filepath).unwrap().modified().unwrap();
                                    let histogram = point_to_histogramm(&data, (0, 400), 400).await;
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

        children.sort_by_key(|e| {
            let path = match e {
                FSRepr::File { path }  => path,
                FSRepr::Directory { path, children: _ } => path
            };
            path.file_name().unwrap().to_str().unwrap().to_owned()
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

            if let Some(root) = &mut self.root {

                if let Ok(ref mut mutex) = self.state.try_lock() {
                    file_tree_entry(ui,root, mutex);
                }
            }
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
                                    format!("ch #{ch_num}")
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
    let (rx, tx) = watch::channel(None::<Action>);

    tokio::spawn(background_processing(tx, configuration));

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "data-viewer",
        options,
        Box::new(|_cc| {
            Box::new(MyApp {
                root: opt.directory.map(expand_dir),
                state,
                background_pipe: rx
            })
        }),
    );
}

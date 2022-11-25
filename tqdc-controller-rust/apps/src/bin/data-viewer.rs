use std::{path::PathBuf, collections::HashMap, sync::Arc};

use protobuf::Message;
use dataforge::protos::rsb_event;
use apps::{point_to_histogramm, PointHistogramm};
use tokio::sync::{watch, Mutex};
use eframe::egui;
use eframe::egui::plot::{Plot, Line};

#[derive(Debug)]
struct FileCache {
    opened: bool,
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
        path: PathBuf
    },
    Directory {
        path: PathBuf,
        children: Vec<FSRepr>
    }
}

fn expand_dir(path: PathBuf) -> FSRepr {
    let meta = std::fs::metadata(&path).unwrap();
    if meta.is_file() {
        FSRepr::File { path }

    } else if meta.is_dir() {
        let children = std::fs::read_dir(&path).unwrap();

        let children = children.map(|child| {
            let entry = child.unwrap();
            expand_dir(entry.path())
        }).collect::<Vec<_>>();

        FSRepr::Directory { path, children }
    } else {
        panic!()
    }
}


#[tokio::main]
async fn main() {

    let state = Arc::new(Mutex::new(HashMap::<String, FileCache>::new()));
    let configuration = Arc::clone(&state);

    let (rx, mut tx) = watch::channel(None::<Action>);
    let options = eframe::NativeOptions::default();

    tokio::spawn(async move {

        while tx.changed().await.is_ok() {
            let action = (*tx.borrow()).unwrap();
            match action {
                Action::CalculateHistogram => {

                    let mut conf = configuration.lock().await;

                    for (filepath, file_cache) in conf.iter_mut() {
                        if file_cache.opened {
                            let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
                            let message = dataforge::extract_df_message(&mut point_file).await.unwrap();
                            let data = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
                            let histogram = point_to_histogramm(&data, (0, 400), 400).await;


                            file_cache.histogram = Some(histogram);
                        }
                    }
                }
            }
        }
    });

    eframe::run_native(
        "data-viewer",
        options,
        Box::new(|_cc| Box::new(MyApp {
            root: None,
            state,
            background_pipe: rx
        })),
    );
}


fn folder_browser(
    ui: &mut egui::Ui, 
    entry: &FSRepr, 
    opened_files: &mut HashMap<String, FileCache>,
) {
    match entry {
        FSRepr::File { path } => {
            let checked = &mut opened_files.entry(path.to_str().unwrap().to_string()).or_insert(FileCache { 
                opened: false, 
                histogram: None 
            }).opened;
            ui.horizontal(|ui| {
                let resp = ui.checkbox(checked, "");
                if resp.changed() && *checked {
                    // cmd_pipe.send(Some(Action::CalculateHistogram(path.to_owned())));
                    println!("{checked}")
                }
                ui.label(path.file_name().unwrap().to_str().unwrap());
            });
        }

        FSRepr::Directory { path, children } => {
            egui::CollapsingHeader::new(path.file_name().unwrap().to_str().unwrap())
            .id_source(path.to_str().unwrap())
            .show(ui, |ui| {
                for child in children {
                    folder_browser(ui, child, opened_files) 
                }
            });
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        
        egui::SidePanel::left("left").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("reload").clicked() {
                    self.root = Some(expand_dir(PathBuf::from("/home/chernov/data/online/output/")))
                }

                if ui.button("apply").clicked() {

                    self.background_pipe.send(Some(Action::CalculateHistogram)).unwrap();
                    println!("AAA");
                    // self.root = Some(expand_dir(PathBuf::from("/home/chernov/data/online/output/")))
                }
            });

            if let Some(root) = &mut self.root {

                if let Ok(ref mut mutex) = self.state.try_lock() {
                    folder_browser(ui,root, mutex);
                } else {
                    // println!("try_lock failed");
                }   
            }
        });

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            if ui.button("draw").clicked() {
                println!("{:?}", self.state)
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {


            if let Ok(state) = self.state.try_lock() {

                for (filepath, hash) in state.iter() {

                    if hash.opened && hash.histogram.is_some() {
                        let hist = hash.histogram.clone().unwrap();
                        let lines = hist.channels.iter().map(|(ch_num, waveform)| {
                            Line::new(
                                waveform.iter().enumerate().map(|(x, y)| [x as f64, *y as f64]).collect::<Vec<_>>()).name(
                                    format!("{ch_num}")
                                )
                        });

                        Plot::new("Test Plot").show(ui, |plot_ui| {
                            lines.for_each(|line| {
                                plot_ui.line(line)
                            })
                        });
                    }
                }
            } else {
                Plot::new("Test Plot").show(ui, |plot_ui| {});
            }
        });
    }
}
use std::path::PathBuf;
use std::{collections::HashMap, sync::Arc};

use eframe::epaint::{Color32, Hsva};

#[cfg(not(target_arch = "wasm32"))]
use {
    tokio:: spawn,
    std::fs::File,
    std::io::Write,
    home::home_dir,
    crate::backend::{process_file, expand_dir},
    which::which,
};

#[cfg(target_arch = "wasm32")]
use {
    wasm_bindgen_futures::spawn_local as spawn,
    gloo_net::http::Request,
    crate::backend::ProcessRequest,
    eframe::web_sys::window,
    wasm_bindgen::prelude::*
};

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    fn download(filename: &str, text: &str);
}

use eframe::egui::{self, Ui};
use eframe::egui::plot::{Plot, Line, Legend};


use egui::mutex::Mutex;
use processing::{Algorithm, ProcessingParams};

use crate::backend::{FSRepr, FileCache};

pub const DEFAULT_LIKHOVID: Algorithm = Algorithm::Likhovid { left: 6, right: 36 };


pub fn color_same_as_egui(idx: usize) -> Color32 {
    let golden_ratio = (5.0_f32.sqrt() - 1.0) / 2.0; // 0.61803398875
    let h = idx as f32 * golden_ratio;
    Hsva::new(h, 0.85, 0.5, 1.0).into() // TODO(emilk): OkLab or some other perspective color space
}

pub struct DataViewerApp {
    pub processing_params: Arc<Mutex<ProcessingParams>>,
    pub root: Arc<Mutex<Option<FSRepr>>>,
    state: Arc<Mutex<HashMap<String, FileCache>>>,
}

impl DataViewerApp {

    pub fn new() -> Self {
        Self {
            root: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(HashMap::new())),
            processing_params: Arc::new(Mutex::new(processing::ProcessingParams {
                algorithm: DEFAULT_LIKHOVID,
                convert_to_kev: true,
                merge_close_events: true,
                use_dead_time: true,
                effective_dead_time: 4000,
                merge_map: [
                    [false, true, false, false, false, false, false],
                    [false, false, false, true, false, false, false],
                    [false, false, false, false, true, false, false],
                    [false, false, false, false, false, false, true],
                    [true, false, false, false, false, false, false],
                    [true, true, true, true, true, false, true],
                    [false, false, true, false, false, false, false],
                ],
                hist_min: 0.0,
                hist_max: 27.0,
                hist_bins: 270
            }))
        }
    }

    fn files_editor(&self, ui: &mut Ui) {
    
        let root_lock = self.root.lock().clone();
    
        ui.horizontal(|ui| {
    
            if ui.button("open").clicked() {
                let root = self.root.clone();
                
                spawn(async move {
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(root_path) = rfd::FileDialog::new().pick_folder() {
                        *root.lock() = Some(expand_dir(root_path))
                    }
                    #[cfg(target_arch = "wasm32")] {
                        let resp = Request::get("/api/files").send().await.unwrap();
                        *root.lock() = Some(resp.json::<FSRepr>().await.unwrap())
                    }
                });
            }
    
            let path = root_lock.clone().map(|root| match root {
                FSRepr::File { path } => path,
                FSRepr::Directory { path, children: _ } => path
            });
            if path.is_some() && ui.button("reload").clicked() {
                #[cfg(not(target_arch = "wasm32"))]
                #[allow(clippy::unnecessary_unwrap)]
                {
                    *self.root.lock() = Some(expand_dir(path.unwrap()));
                }
                #[cfg(target_arch = "wasm32")]{
                    let root = self.root.clone();
                    spawn(async move {
                        let resp = Request::get("/api/files").send().await.unwrap();
                        *root.lock() = Some(resp.json::<FSRepr>().await.unwrap())
                    })
                }
            }
    
            if ui.button("apply").clicked() {
                self.process()
            }
    
            if ui.button("clear").clicked() {
                self.state.lock().clear()
            }
            
            if ui.button("save").clicked() {
                let state = self.state.lock().clone();

                spawn(async move {
                    #[cfg(not(target_arch = "wasm32"))]
                    let save_folder = rfd::FileDialog::new().set_directory(home_dir().unwrap()).pick_folder();
                    #[cfg(target_arch = "wasm32")]
                    let save_folder = Some(PathBuf::new());

                    if let Some(save_folder) = save_folder {
    
                        for (name, cache) in state.iter() {

                            if let Some(histogramm) = &cache.histogram {
                                
                                let point_name = {
                                    let temp = PathBuf::from(name);
                                    temp.file_name().unwrap().to_owned()
                                };
        
                                let mut data = String::new();
        
                                {
                                    let mut row = String::new();
                                    row.push_str("bin\t");
                                    for ch_num in histogramm.channels.keys() {
                                        row.push_str(&format!("ch {}\t", *ch_num + 1));
                                    }
                                    row.push('\n');
            
                                    data.push_str(&row);
                                }
        
                                for (idx, bin) in  histogramm.x.iter().enumerate() {
        
                                    let mut row = String::new();
        
                                    row.push_str(&format!("{bin:.4}\t"));
                                    for val in histogramm.channels.values() {
                                        row.push_str(&format!("{}\t", val[idx]));
                                    }
                                    row.push('\n');
                                    data.push_str(&row);
                                }
        
                                let mut filepath = save_folder.clone();
                                filepath.push(point_name);

                                #[cfg(not(target_arch = "wasm32"))]
                                {
                                    let mut out_file = File::create(filepath).unwrap();
                                    out_file.write_all(data.as_bytes()).unwrap();
                                }
                                #[cfg(target_arch = "wasm32")]
                                download(filepath.to_str().unwrap(), &data);
                            }
                        }
                    }
                });
            }
        });
    
        egui::containers::ScrollArea::new([false, true]).show(ui, |ui| {
            if let Some(root) = &root_lock {
                file_tree_entry(ui, root, &mut self.state.lock());
            }
        });
    }

    pub fn process(&self) {

        let params = *self.processing_params.lock();
        let state = Arc::clone(&self.state);

        {
            spawn(async move {
                // TODO: add conditional recalculation
                // let mut last_params = *processing_params.lock().await;
                // let need_recalc = if last_params != params {
                //     last_params = params;
                //     true
                // } else {
                //     false
                // };
    
                let files_to_processed = {
                    state.lock().iter().filter_map(|(filepath, cache)| {
                        if cache.opened {
                            let need_recalc = true;
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
                    let configuration_local = state.clone();
                    spawn(async move {
                        #[cfg(not(target_arch = "wasm32"))] let cache = {
                            process_file(PathBuf::from(&filepath), params).await
                        };
                        #[cfg(target_arch = "wasm32")] let cache = {
                            Request::post("/api/process").json(&ProcessRequest::CalcHist {
                                filepath: PathBuf::from(&filepath),
                                params
                            }).unwrap().send().await.unwrap().json::<FileCache>().await.unwrap()
                        };
                        let mut conf = configuration_local.lock();
                        conf.insert(filepath.to_owned(), cache);
                    });
                }
                
            });
        }
    }

}

impl Default for DataViewerApp {
    fn default() -> Self {
        Self::new()
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


fn params_editor(ui: &mut Ui, processing_params: ProcessingParams) -> ProcessingParams {

    let mut algorithm = processing_params.algorithm;

    ui.horizontal(|ui| {
        if ui.add(egui::RadioButton::new(algorithm == Algorithm::Max, "Max")).clicked() {
            algorithm = Algorithm::Max
        }
        
        if ui.add(egui::RadioButton::new(
            matches!(algorithm, Algorithm::Likhovid { .. }), "Likhovid")).clicked() {
            algorithm = DEFAULT_LIKHOVID
        }
    });

    if let Algorithm::Likhovid { left, right } = algorithm {

        ui.separator();
        ui.label("Algorithm params");

        let mut left = left;
        ui.add(egui::Slider::new(&mut left, 0..=15).text("left"));
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

    
    let mut use_dead_time = processing_params.use_dead_time;
    let mut effective_dead_time = processing_params.effective_dead_time;

    ui.checkbox(&mut use_dead_time, "use dead time");
    ui.add_enabled(use_dead_time, 
        egui::Slider::new(&mut effective_dead_time, 0..=10000).text("ns")
    );

    let mut merge_close_events = processing_params.merge_close_events;
    ui.checkbox(&mut merge_close_events, "merge close events");
    

    let mut merge_map = processing_params.merge_map;
    ui.collapsing("merge mapping", |ui| {

        egui_extras::TableBuilder::new(ui)
        // .auto_shrink([false, false])
        .columns(egui_extras::Column::initial(15.0), 8)
        .header(20.0, |mut header| {
            header.col(|_| {});
            for idx in 0..7 {
                header.col(|ui| {
                    ui.label((idx + 1).to_string());
                });
            }
        })
        .body(|mut body| {
            for ch_1 in 0usize..7 {
                body.row(20.0, |mut row| {
                    row.col(|ui| { ui.label(format!("{}<", ch_1 + 1));});
                    for ch_2 in 0usize..7 {
                        row.col(|ui| {
                            if ch_1 == ch_2 {
                                let checkbox = egui::Checkbox::new(&mut merge_map[ch_1][ch_2], "");
                                ui.add_enabled(false, checkbox);
                            } else if ui.checkbox(&mut merge_map[ch_1][ch_2], "").changed() && merge_map[ch_1][ch_2] {
                                merge_map[ch_2][ch_1] = false;
                            }
                        });
                    }
                });
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        {
            // TODO: add image to web
            let image = egui_extras::image::RetainedImage::from_image_bytes(
            "Detector.drawio.png", 
            include_bytes!("../resources/Detector.drawio.png")).unwrap();
            image.show(ui);
        }
    });
    ui.separator();

    ProcessingParams {
        algorithm,
        convert_to_kev,
        hist_min,
        hist_max,
        use_dead_time,
        effective_dead_time,
        hist_bins,
        merge_close_events,
        merge_map,
    }
}

impl eframe::App for DataViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        ctx.request_repaint_after(std::time::Duration::from_secs(1));

        egui::SidePanel::left("left").show(ctx, |ui| {
            let mut processing_params = self.processing_params.lock();
            *processing_params = params_editor(ui, *processing_params);
            drop(processing_params);
            self.files_editor(ui);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let state = self.state.lock();
                
            let mut left_border = 0.0;
            let mut right_border = 0.0;

            let opened_files = state.iter().filter(|(_, cache)| {
                cache.opened
            }).collect::<Vec<_>>();

            #[cfg(not(target_arch = "wasm32"))]
            let height = _frame.info().window_info.size.y;
            #[cfg(target_arch = "wasm32")]
            let height = window().unwrap().inner_height().unwrap().as_f64().unwrap() as f32;

            
            let plot = Plot::new("Histogram Plot").legend(Legend { 
                text_style: egui::TextStyle::Body, 
                background_alpha: 1.0, position: egui::plot::Corner::RightTop 
            })
            .height(height - 35.0);

            plot.show(ui, |plot_ui| {

                let bounds = plot_ui.plot_bounds();
                left_border = bounds.min()[0] as f32;
                right_border = bounds.max()[0] as f32;
            
                let lines = if opened_files.len() == 1 {
                    let (_, cache) = opened_files[0];
                    if !(cache.opened && cache.histogram.is_some()) {
                        return;
                    }
                    let hist = cache.histogram.clone().unwrap();
                    hist.channels.iter().map(|(ch_num, y)| {
                        (format!("ch #{}", ch_num + 1), color_same_as_egui(*ch_num as usize), hist.step, hist.x.clone(), y.clone())
                    }).collect::<Vec<_>>()
                } else {
                    opened_files.iter().enumerate()
                    .filter(|(_, (_, cache))| {cache.histogram.is_some()})
                    .map(|(idx, (filepath, cache))| {
                        let hist = cache.histogram.clone().unwrap();

                        let mut y_all = vec![0.0; hist.x.len()];
                        for (_, y) in hist.channels {
                            for (idx, val) in y.iter().enumerate() {
                                y_all[idx] += val;
                            }
                        }

                        (filepath.to_string(), color_same_as_egui(idx), hist.step, hist.x, y_all)
                    }).collect::<Vec<_>>()
                };

                for (name, color, step, x, y) in lines {
                    let mut events_in_window = 0;

                    let line_data = y.iter().enumerate().flat_map(|(idx, y)| {
                        if x[idx] > left_border && x[idx] < right_border {
                            events_in_window += *y as i32;
                        }
                        [
                            [(x[idx] - step / 2.0)  as f64, *y as f64],
                            [(x[idx] + step / 2.0)  as f64, *y as f64]
                        ]
                    }).collect::<Vec<_>>();

                    plot_ui.line(Line::new(line_data)
                    .width(if ctx.style().visuals.dark_mode { 1.0 } else { 2.0 })
                    .color(color)
                    .name(
                        format!("{name}\t({events_in_window})")
                    ))
                }
            });

            // TODO: move to separate function
            
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {

                #[cfg(not(target_arch = "wasm32"))]
                let filtered_viewer_in_path = which("filtered-viewer").is_ok();
                #[cfg(target_arch = "wasm32")]
                let filtered_viewer_in_path = true;
                
                let algorithm_ok = {
                    let params = self.processing_params.lock();
                    let algorithm_ok = params.algorithm == Algorithm::Likhovid { left: 6, right: 36 } && params.convert_to_kev;
                    drop(params);
                    algorithm_ok
                };

                let filtered_viewer_button = ui.add_enabled(
                    opened_files.len() == 1 && filtered_viewer_in_path && algorithm_ok,  
                    egui::Button::new("waveforms (in window)")).on_disabled_hover_ui(|ui| {
                    if !filtered_viewer_in_path {
                        ui.colored_label(Color32::RED, "filtered-viewer must be in PATH");
                    }
                    if opened_files.len() != 1 {
                        ui.colored_label(Color32::RED, "exact one file must be opened");
                    }
                    if !algorithm_ok {
                        ui.colored_label(Color32::RED, "params must be default (Algorithm::Likhovid { left: 6, right: 36 }, convert_to_kev)");
                    }
                });

                if filtered_viewer_button.clicked() {
                    let (filepath, _) = opened_files[0];
                    #[cfg(not(target_arch = "wasm32"))] {
                        tokio::process::Command::new("filtered-viewer").arg(filepath)
                        .arg("--min").arg(left_border.max(0.0).to_string())
                        .arg("--max").arg(right_border.max(0.0).to_string())
                        .spawn().unwrap();
                    }
                    #[cfg(target_arch = "wasm32")] {
                        let search = serde_qs::to_string(&ProcessRequest::FilterEvents { 
                            filepath: PathBuf::from(filepath), 
                            range: left_border.max(0.0)..right_border.max(0.0), 
                            neigborhood: 5000 }).unwrap();
                        window().unwrap().open_with_url(&format!("/?{search}")).unwrap();
                    }
                }

                #[cfg(not(target_arch = "wasm32"))]
                let point_viewer_in_path = which("point-viewer").is_ok();
                #[cfg(target_arch = "wasm32")]
                let point_viewer_in_path = true;
                
                let point_viewer_button = ui.add_enabled(opened_files.len() == 1 && point_viewer_in_path,  
                egui::Button::new("waveforms (all)")).on_disabled_hover_ui(|ui| {
                    if !point_viewer_in_path {
                        ui.colored_label(Color32::RED, "point-viewer must be in PATH");
                    }
                    if opened_files.len() != 1 {
                        ui.colored_label(Color32::RED, "exact one file must be opened");
                    }
                });

                if point_viewer_button.clicked() {
                    let (filepath, _) = opened_files[0];
                    #[cfg(not(target_arch = "wasm32"))] {
                        tokio::process::Command::new("point-viewer").arg(filepath).spawn().unwrap();
                    }
                    #[cfg(target_arch = "wasm32")] {
                        let search = serde_qs::to_string(&ProcessRequest::SplitTimeChunks {
                            filepath: PathBuf::from(filepath)
                        }).unwrap();
                        window().unwrap().open_with_url(&format!("/?{search}")).unwrap();
                    }
                }
            });
        });
    }
}

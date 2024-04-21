//! This module contains egui widgets for processing configurations

use crate::{histogram::HistogramParams, postprocess::PostProcessParams, process::{Algorithm, HWResetParams, ProcessParams}, process::{FIRSTPEAK_DEFAULT, LIKHOVID_DEFAULT, TRAPEZOID_DEFAULT}};

pub trait UserInput {
    // draw input form and read changes from it
    fn input(&self, ui: &mut egui::Ui, ctx: &egui::Context) -> Self;
}

impl UserInput for HistogramParams {
    fn input(&self, ui: &mut egui::Ui, _: &egui::Context) -> Self {
        ui.label("Histogram params");
        let mut min = self.range.start;
        ui.add(egui::Slider::new(&mut min, -10.0..=400.0).text("left"));
        let mut max = self.range.end;
        ui.add(egui::Slider::new(&mut max, -10.0..=400.0).text("right"));
        let mut bins = self.bins;
        ui.add(egui::Slider::new(&mut bins, 10..=2000).text("bins"));

        HistogramParams { range: min..max, bins }
    }
}

impl UserInput for ProcessParams {
    fn input(&self, ui: &mut egui::Ui,  _: &egui::Context) -> Self {
        let mut algorithm = self.algorithm.to_owned();

        ui.label("Processing params");

        ui.horizontal(|ui| {
            if ui
                .add(egui::RadioButton::new(algorithm == Algorithm::Max, "Max"))
                .clicked()
            {
                algorithm = Algorithm::Max
            }

            if ui
                .add(egui::RadioButton::new(
                    matches!(algorithm, Algorithm::Likhovid { .. }),
                    "Likhovid",
                ))
                .clicked()
            {
                algorithm = LIKHOVID_DEFAULT
            }

            if ui
                .add(egui::RadioButton::new(
                    matches!(algorithm, Algorithm::FirstPeak { .. }),
                    "FirstPeak",
                ))
                .clicked()
            {
                algorithm = FIRSTPEAK_DEFAULT
            }
        });

        ui.horizontal(|ui| {
            if ui
                .add(egui::RadioButton::new(
                    matches!(algorithm, Algorithm::Trapezoid { .. }),
                    "Trapezoid",
                ))
                .clicked()
            {
                algorithm = TRAPEZOID_DEFAULT
            }
        });

        let algorithm = match algorithm {
            Algorithm::Max => {
                algorithm
            }
            Algorithm::Likhovid { left, right } => {
                let mut left = left;
                ui.add(egui::Slider::new(&mut left, 0..=30).text("left"));
                let mut right = right;
                ui.add(egui::Slider::new(&mut right, 0..=40).text("right"));

                Algorithm::Likhovid { left, right }
            }
            Algorithm::FirstPeak { threshold, left } => {

                let mut left = left;
                ui.add(egui::Slider::new(&mut left, 0..=30).text("left"));

                let mut threshold = threshold;
                ui.add(egui::Slider::new(&mut threshold, 0..=400).text("threshold"));
                Algorithm::FirstPeak { threshold, left }
            }

            Algorithm::Trapezoid { 
                left, center, right, 
                treshold, min_length,
                reset_detection: HWResetParams { 
                    window: r_window, treshold: r_treshold, size: r_size } } => {

                ui.label("sliding window");

                let mut left = left;
                ui.add(egui::Slider::new(&mut left, 0..=32).text("left"));

                let mut center = center;
                ui.add(egui::Slider::new(&mut center, 0..=32).text("center"));

                let mut right = right;
                ui.add(egui::Slider::new(&mut right, 0..=32).text("right"));

                ui.label("extraction");

                let mut treshold = treshold;
                ui.add(egui::Slider::new(&mut treshold, 0..=100).text("treshold"));

                let mut min_length = min_length;
                ui.add(egui::Slider::new(&mut min_length, 0..=100).text("min length"));
                

                ui.label("hw reset detection");

                let mut r_window = r_window;
                ui.add(egui::Slider::new(&mut r_window, 0..=100).text("diff window"));

                let mut r_treshold = r_treshold;
                ui.add(egui::Slider::new(&mut r_treshold, 0..=2000).text("diff treshold"));

                let mut r_size = r_size;
                ui.add(egui::Slider::new(&mut r_size, 0..=500).text("reset size"));
                

                Algorithm::Trapezoid { 
                    left, center, right, 
                    treshold, min_length,
                    reset_detection: HWResetParams { 
                        window: r_window, treshold: r_treshold, size: r_size }}
            }
        };

        let mut convert_to_kev = self.convert_to_kev;
        ui.checkbox(&mut convert_to_kev, "convert to keV");

        ProcessParams { algorithm, convert_to_kev }
    }
}


impl UserInput for PostProcessParams {
    fn input(&self, ui: &mut egui::Ui, ctx: &egui::Context) -> Self {

        let mut use_dead_time = self.use_dead_time;
        let mut effective_dead_time = self.effective_dead_time;
        let mut merge_close_events = self.merge_close_events;
        let mut merge_map = self.merge_map;
    
        ui.add_enabled_ui(false, |ui| { // TODO: fix this
            ui.label("Postprocessing params");
    
            ui.checkbox(&mut use_dead_time, "use dead time");
            ui.add_enabled(
                use_dead_time,
                egui::Slider::new(&mut effective_dead_time, 0..=30000).text("ns"),
            );
            
            ui.checkbox(&mut merge_close_events, "merge close events");
            
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
                                row.col(|ui| {
                                    ui.label(format!("{}<", ch_1 + 1));
                                });
                                for ch_2 in 0usize..7 {
                                    row.col(|ui| {
                                        if ch_1 == ch_2 {
                                            let checkbox =
                                                egui::Checkbox::new(&mut merge_map[ch_1][ch_2], "");
                                            ui.add_enabled(false, checkbox);
                                        } else if ui.checkbox(&mut merge_map[ch_1][ch_2], "").changed()
                                            && merge_map[ch_1][ch_2]
                                        {
                                            merge_map[ch_2][ch_1] = false;
                                        }
                                    });
                                }
                            });
                        }
                    });
    
                let image = if ctx.style().visuals.dark_mode {
                    egui_extras::image::RetainedImage::from_svg_bytes(
                        "Detector.drawio.png",
                        include_bytes!("../resources/detector_dark.svg"),
                    ).unwrap()
                } else {
                    egui_extras::image::RetainedImage::from_svg_bytes(
                        "Detector.drawio.png",
                        include_bytes!("../resources/detector_light.svg"),
                    ).unwrap()
                };
    
                image.show(ui);
            });
        });
        
    
        ui.set_enabled(true);
    
        PostProcessParams { 
            use_dead_time,
            effective_dead_time,
            merge_close_events,
            merge_map
        }
    }
}
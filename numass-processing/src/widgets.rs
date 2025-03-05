//! This module contains egui widgets for processing configurations

use crate::{
    histogram::HistogramParams, postprocess::PostProcessParams, preprocess::{CHECK_BIN_SIZE, CHECK_HV_THRESHOLD, CUTOFF_BIN_SIZE}, process::{
        Algorithm, HWResetParams, ProcessParams, FIRSTPEAK_DEFAULT, LIKHOVID_DEFAULT,
        LONGDIFF_DEFAULT, TRAPEZOID_DEFAULT,
    }
};

pub trait UserInput {
    // draw input form and read changes from it
    fn input(&self, ui: &mut egui::Ui, ctx: &egui::Context) -> Self;
}

impl UserInput for HistogramParams {
    fn input(&self, ui: &mut egui::Ui, _: &egui::Context) -> Self {
        ui.label("Histogram params");
        let mut min = self.range.start;
        ui.add(egui::Slider::new(&mut min, -10.0..=800.0).text("left"));
        let mut max = self.range.end;
        ui.add(egui::Slider::new(&mut max, -10.0..=800.0).text("right"));
        let mut bins = self.bins;
        ui.add(egui::Slider::new(&mut bins, 10..=2000).text("bins"));

        HistogramParams {
            range: min..max,
            bins,
        }
    }
}

impl UserInput for ProcessParams {
    fn input(&self, ui: &mut egui::Ui, _: &egui::Context) -> Self {
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
            if ui
                .add(egui::RadioButton::new(
                    matches!(algorithm, Algorithm::LongDiff { .. }),
                    "LongDiff",
                ))
                .clicked()
            {
                algorithm = LONGDIFF_DEFAULT
            }
        });

        let algorithm = match algorithm {
            Algorithm::Max => algorithm,
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
                left,
                center,
                right,
                treshold,
                min_length,
                skip,
                reset_detection:
                    HWResetParams {
                        window: r_window,
                        treshold: r_treshold,
                        size: r_size,
                    },
            } => {
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

                let mut skip = skip;
                ui.horizontal(|ui| {
                    ui.label("skip:");
                    ui.radio_value(&mut skip, crate::process::SkipOption::None, "none");
                    ui.radio_value(&mut skip, crate::process::SkipOption::Bad, "bad");
                    ui.radio_value(&mut skip, crate::process::SkipOption::Good, "good");
                });

                Algorithm::Trapezoid {
                    left,
                    center,
                    right,
                    treshold,
                    min_length,
                    skip,
                    reset_detection: HWResetParams {
                        window: r_window,
                        treshold: r_treshold,
                        size: r_size,
                    },
                }
            }
            Algorithm::LongDiff {
                reset_detection:
                    HWResetParams {
                        window,
                        treshold,
                        size,
                    },
            } => {
                ui.label("hw reset detection");

                let mut window = window;
                ui.add(egui::Slider::new(&mut window, 0..=100).text("diff window"));

                let mut treshold = treshold;
                ui.add(egui::Slider::new(&mut treshold, 0..=2000).text("diff treshold"));

                let mut size = size;
                ui.add(egui::Slider::new(&mut size, 0..=500).text("reset size"));

                Algorithm::LongDiff {
                    reset_detection: HWResetParams {
                        window,
                        treshold,
                        size,
                    },
                }
            }
        };

        let mut convert_to_kev = self.convert_to_kev;
        ui.checkbox(&mut convert_to_kev, "convert to keV");

        ProcessParams {
            algorithm,
            convert_to_kev,
        }
    }
}

impl UserInput for PostProcessParams {
    fn input(&self, ui: &mut egui::Ui, ctx: &egui::Context) -> Self {
        let mut cut_bad_blocks = self.cut_bad_blocks;
        let mut merge_splits_first = self.merge_splits_first;
        let mut merge_close_events = self.merge_close_events;
        let mut ignore_borders = self.ignore_borders;
        let mut ignore_channels = self.ignore_channels.to_owned();

        ui.add_enabled_ui(true, |ui| {
            ui.label("Postprocessing params");

            ui.checkbox(&mut cut_bad_blocks, "cut_bad_blocks").on_hover_text(
                format!("
                point is splitted into {} s length blocks 
                each block is also be splitted into {} s subblock
                the block is considered bad if any of its subblocks contains 0 events
                the cutting applies only on points with HV < {} keV
                ", 
                    CUTOFF_BIN_SIZE  as f32 * 1e-9,
                    CHECK_BIN_SIZE as f32 * 1e-9,
                    CHECK_HV_THRESHOLD * 1e-3
                )
            );

            ui.checkbox(&mut merge_splits_first, "merge splits first")
                .on_hover_text(
                    "
                    spit - neighboring events with time delta < 200 ns
                    spits will be merged into one event in random order
                    (based on which one is first)
                    this will be processed before main merging
                    "
                );
            ui.checkbox(&mut merge_close_events, "merge close events")
                .on_hover_text(
                    "
                    Merging scheme:
                    idx_1 - An event into which other events merge (goes from first to end)
                    idx_2 - The event that will be merged into idx_1 (goes from end to idx_1)
                    each idx_2.ch_id checks for neigborhooding with idx_1.ch_id
                    if idx_1 and idx_2 are neigbors => idx_1.amp += idx_2.amp and idx_2 will be removed
                    "
                );
            ui.checkbox(&mut ignore_borders, "ignore borders").on_hover_text("
            do not check for neigboorhooding at merging step
            with this flag every event in frame will be merged into first one
            ");

            ui.collapsing("merge mapping", |ui| {
                let image = if ctx.style().visuals.dark_mode {
                    egui::include_image!("../resources/detector_dark.svg")
                } else {
                    egui::include_image!("../resources/detector_light.svg")
                };
                ui.image(image);
            });

            ui.collapsing("ignore channels", |ui| {
                ignore_channels
                .iter_mut()
                .enumerate()
                .collect::<Vec<_>>()
                .chunks_mut(3)
                .for_each(|chunk| {
                    ui.horizontal(|ui| {
                        chunk.iter_mut().for_each(|(idx, ignored)| {
                            let idx_readable = idx.to_owned() + 1;
                            ui.checkbox(ignored, &format!("{idx_readable}"));
                        });
                    });
                });
            });
        });

        PostProcessParams {
            cut_bad_blocks,
            merge_splits_first,
            merge_close_events,
            ignore_borders,
            ignore_channels,
        }
    }
}

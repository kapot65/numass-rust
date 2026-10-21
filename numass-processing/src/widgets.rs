//! This module contains egui widgets for processing configurations

use crate::{
    histogram::HistogramParams,
    postprocess::PostProcessParams,
    preprocess::{
        AVERAGE_SCRIPT, Baseline, CHECK_BIN_SIZE, CHECK_HV_THRESHOLD, CUTOFF_BIN_SIZE, FIR_CENTER,
        FIR_LEFT, FIR_RIGHT, HEAD_BINS, POINT_BASELINE_MAX, POINT_BASELINE_MIN,
        POINT_BASELINE_STEP, PreprocessParams, PreprocessStep,
    },
    process::{
        Algorithm, FIRSTPEAK_DEFAULT, HWResetParams, LIKHOVID_DEFAULT, LONGDIFF_DEFAULT,
        ProcessParams, TRAPEZOID_DEFAULT,
    },
};

pub const HISTOGRAM_MIN: f32 = -200.0;
pub const HISTOGRAM_MAX: f32 = 800.0;

pub trait UserInput {
    // draw input form and read changes from it
    fn input(&self, ui: &mut egui::Ui, ctx: &egui::Context) -> Self;
}

/// Collapsible section.
///
/// `body` runs only while the section is open, including the opening animation.
/// Widgets that edit state belong in `body`: a closed section leaves that state unchanged.
/// `header_extra` is always visible, on the same row as the title.
fn collapsing<R>(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    title: &str,
    default_open: bool,
    header_extra: impl FnOnce(&mut egui::Ui) -> R,
    body: impl FnOnce(&mut egui::Ui),
) -> R {
    let id = ui.make_persistent_id(id);
    let mut toggle_label = false;
    let mut header = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        id,
        default_open,
    )
    .show_header(ui, |ui| {
        let extra = header_extra(ui);
        if ui
            .add(egui::Label::new(title).sense(egui::Sense::click()))
            .clicked()
        {
            toggle_label = true;
        }
        extra
    });
    if toggle_label {
        header.toggle();
    }
    let (_, extra, _) = header.body(body);
    extra.inner
}

fn block(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    collapsing(ui, title, title, true, |_| {}, body);
}

fn block_closed(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    collapsing(ui, title, title, false, |_| {}, body);
}

fn step_title(step: &PreprocessStep) -> &'static str {
    match step {
        PreprocessStep::Fir { .. } => "FIR",
        PreprocessStep::Baseline(Baseline::Constant(_)) => "baseline const",
        PreprocessStep::Baseline(Baseline::Head { .. }) => "baseline head",
        PreprocessStep::Baseline(Baseline::Point { .. }) => "baseline point",
        PreprocessStep::Invert {} => "invert",
        PreprocessStep::Merge {} => "merge",
        PreprocessStep::Script { .. } => "script",
    }
}

fn hw_reset_block(ui: &mut egui::Ui, reset: &mut HWResetParams) {
    block(ui, "hw reset detection", |ui| {
        ui.add(egui::Slider::new(&mut reset.window, 0..=100).text("diff window"));
        ui.add(egui::Slider::new(&mut reset.treshold, 0..=2000).text("diff treshold"));
        ui.add(egui::Slider::new(&mut reset.size, 0..=500).text("reset size"));
    });
}

impl UserInput for HistogramParams {
    fn input(&self, ui: &mut egui::Ui, _: &egui::Context) -> Self {
        let mut params = self.clone();
        block(ui, "Histogram params", |ui| {
            ui.add(
                egui::Slider::new(&mut params.range.start, HISTOGRAM_MIN..=HISTOGRAM_MAX)
                    .text("left"),
            );
            ui.add(
                egui::Slider::new(&mut params.range.end, HISTOGRAM_MIN..=HISTOGRAM_MAX)
                    .text("right"),
            );
            ui.add(egui::Slider::new(&mut params.bins, 10..=2000).text("bins"));
        });
        params
    }
}

impl UserInput for PreprocessParams {
    fn input(&self, ui: &mut egui::Ui, _: &egui::Context) -> Self {
        let mut steps = self.steps.clone();

        block(ui, "Preprocess", |ui| {
            let mut remove = None;

            for (idx, step) in steps.iter_mut().enumerate() {
                let title = step_title(step);
                let removed = collapsing(
                    ui,
                    ("preprocess-step", idx),
                    title,
                    true,
                    |ui| ui.button("x").clicked(),
                    |ui| match step {
                        PreprocessStep::Fir {
                            left,
                            center,
                            right,
                        } => {
                            ui.add(egui::Slider::new(left, 0..=32).text("left"));
                            ui.add(egui::Slider::new(center, 0..=32).text("center"));
                            ui.add(egui::Slider::new(right, 0..=32).text("right"));
                        }
                        PreprocessStep::Baseline(Baseline::Constant(values)) => {
                            for (channel, value) in values.iter_mut().enumerate() {
                                ui.add(
                                    egui::Slider::new(value, -500.0..=2000.0)
                                        .text(format!("ch{}", channel + 1)),
                                );
                            }
                        }
                        PreprocessStep::Baseline(Baseline::Head { bins }) => {
                            ui.add(egui::Slider::new(bins, 1..=200).text("bins"));
                        }
                        PreprocessStep::Baseline(Baseline::Point { min, max, step }) => {
                            ui.add(egui::Slider::new(min, -200.0..=800.0).text("min"));
                            ui.add(egui::Slider::new(max, -200.0..=800.0).text("max"));
                            ui.add(egui::Slider::new(step, 0.05..=20.0).text("step"));
                        }
                        PreprocessStep::Invert {} => {}
                        PreprocessStep::Merge {} => {}
                        PreprocessStep::Script { source } => {
                            ui.label("Luau: `function(frame)`; `frame[ch].samples` — отсчёты с индексацией от 1.")
                                .on_hover_text(
                                    "
                                Скрипт возвращает function(frame) и меняет кадр на месте.
                                Ключ кадра — id канала (0-based), у канала поля samples и delay.
                                Глобалы: point (hv, acquisition_time, frame_len, start_time, bad_blocks)
                                и host (функции, зарегистрированные при сборке VM).
                                ",
                                );
                            ui.add(
                                egui::TextEdit::multiline(source)
                                    .code_editor()
                                    .desired_rows(12)
                                    .desired_width(f32::INFINITY),
                            );
                        }
                    },
                );
                if removed {
                    remove = Some(idx);
                }
            }

            if let Some(idx) = remove {
                steps.remove(idx);
            }

            ui.horizontal(|ui| {
                if ui.button("+ FIR").clicked() {
                    steps.push(PreprocessStep::Fir {
                        left: FIR_LEFT,
                        center: FIR_CENTER,
                        right: FIR_RIGHT,
                    });
                }
                if ui.button("+ head").clicked() {
                    steps.push(PreprocessStep::Baseline(Baseline::Head { bins: HEAD_BINS }));
                }
                if ui.button("+ point").clicked() {
                    steps.push(PreprocessStep::Baseline(Baseline::Point {
                        min: POINT_BASELINE_MIN,
                        max: POINT_BASELINE_MAX,
                        step: POINT_BASELINE_STEP,
                    }));
                }
                if ui.button("+ const").clicked() {
                    steps.push(PreprocessStep::Baseline(Baseline::Constant([0.0; 7])));
                }
                if ui.button("+ invert").clicked() {
                    steps.push(PreprocessStep::Invert {});
                }
                if ui.button("+ merge").clicked() {
                    steps.push(PreprocessStep::Merge {});
                }
                if ui.button("+ script").clicked() {
                    steps.push(PreprocessStep::Script {
                        source: AVERAGE_SCRIPT.to_owned(),
                    });
                }
            });
        });

        PreprocessParams { steps }
    }
}

impl UserInput for ProcessParams {
    fn input(&self, ui: &mut egui::Ui, _: &egui::Context) -> Self {
        let mut algorithm = self.algorithm.to_owned();
        let mut convert_to_kev = self.convert_to_kev;

        block(ui, "Processing params", |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add(egui::RadioButton::new(
                        algorithm == Algorithm::Max {},
                        "Max",
                    ))
                    .clicked()
                {
                    algorithm = Algorithm::Max {}
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
                if ui
                    .add(egui::RadioButton::new(
                        matches!(algorithm, Algorithm::Average {}),
                        "Average",
                    ))
                    .clicked()
                {
                    algorithm = Algorithm::Average {}
                }
            });

            match &mut algorithm {
                Algorithm::Max {} | Algorithm::Average {} => {}
                Algorithm::Likhovid { left, right } => {
                    ui.add(egui::Slider::new(left, 0..=30).text("left"));
                    ui.add(egui::Slider::new(right, 0..=40).text("right"));
                }
                Algorithm::FirstPeak { threshold, left } => {
                    ui.add(egui::Slider::new(left, 0..=30).text("left"));
                    ui.add(egui::Slider::new(threshold, 0..=400).text("threshold"));
                }
                Algorithm::Trapezoid {
                    treshold,
                    min_length,
                    skip,
                    reset_detection,
                } => {
                    block(ui, "extraction", |ui| {
                        ui.add(egui::Slider::new(treshold, 0..=100).text("treshold"));
                        ui.add(egui::Slider::new(min_length, 0..=100).text("min length"));
                    });
                    hw_reset_block(ui, reset_detection);
                    ui.horizontal(|ui| {
                        ui.label("skip:");
                        ui.radio_value(skip, None, "none");
                        ui.radio_value(skip, Some(crate::process::SkipOption::Bad), "bad");
                        ui.radio_value(skip, Some(crate::process::SkipOption::Good), "good");
                    });
                }
                Algorithm::LongDiff { reset_detection } => {
                    hw_reset_block(ui, reset_detection);
                }
            }

            ui.checkbox(&mut convert_to_kev, "convert to keV");
        });

        ProcessParams {
            algorithm,
            convert_to_kev,
        }
    }
}

impl UserInput for PostProcessParams {
    fn input(&self, ui: &mut egui::Ui, ctx: &egui::Context) -> Self {
        let mut cut_bad_blocks = self.cut_bad_blocks;
        let mut merge_frames = self.merge_frames;
        let mut merge_splits_first = self.merge_splits_first;
        let mut merge_close_events = self.merge_close_events;
        let mut ignore_borders = self.ignore_borders;
        let mut ignore_channels: [bool; 7] = self.ignore_channels.to_owned().into();

        block(ui, "Postprocessing params", |ui| {
            ui.checkbox(&mut cut_bad_blocks, "cut_bad_blocks")
                .on_hover_text(format!(
                    "
                point is splitted into {} s length blocks
                each block is also be splitted into {} s subblock
                the block is considered bad if any of its subblocks contains 0 events
                the cutting applies only on points with HV < {} keV
                ",
                    CUTOFF_BIN_SIZE as f32 * 1e-9,
                    CHECK_BIN_SIZE as f32 * 1e-9,
                    CHECK_HV_THRESHOLD * 1e-3
                ));

            ui.horizontal(|ui| {
                let mut merge_frames_checked = merge_frames.is_some();
                if ui
                    .checkbox(&mut merge_frames_checked, "merge frames")
                    .on_hover_text(
                        "
                    Merge events between close frames into one.
                    Loop goes from end to start merging any frame to previous one
                    if the distance between them is less than treshold.
                    This operation applies before any other postprocessing.
                    "
                        .to_string(),
                    )
                    .clicked()
                {
                    if merge_frames_checked {
                        merge_frames = Some(5000);
                    } else {
                        merge_frames = None;
                    }
                }

                if merge_frames_checked {
                    let mut merge_frames_value = merge_frames.unwrap();
                    if ui
                        .add(egui::Slider::new(&mut merge_frames_value, 1000..=65535).text("ns"))
                        .changed()
                    {
                        merge_frames = Some(merge_frames_value);
                    }
                }
            });

            ui.checkbox(&mut merge_splits_first, "merge splits first")
                .on_hover_text(
                    "
                    split - neighboring events with time delta < 200 ns
                    splits will be merged into one event in random order
                    (based on which one is first)
                    this will be processed before main merging
                    ",
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
            ui.checkbox(&mut ignore_borders, "ignore borders")
                .on_hover_text(
                    "
            do not check for neigboorhooding at merging step
            with this flag every event in frame will be merged into first one
            ",
                );

            block_closed(ui, "merge mapping", |ui| {
                let image = if ctx.style().visuals.dark_mode {
                    egui::include_image!("../resources/detector_dark.svg")
                } else {
                    egui::include_image!("../resources/detector_light.svg")
                };
                ui.image(image);
            });

            block_closed(ui, "ignore channels", |ui| {
                ignore_channels
                    .iter_mut()
                    .enumerate()
                    .collect::<Vec<_>>()
                    .chunks_mut(3)
                    .for_each(|chunk| {
                        ui.horizontal(|ui| {
                            chunk.iter_mut().for_each(|(idx, ignored)| {
                                let idx_readable = idx.to_owned() + 1;
                                ui.checkbox(ignored, format!("{idx_readable}"));
                            });
                        });
                    });
            });
        });

        PostProcessParams {
            cut_bad_blocks,
            merge_frames,
            merge_splits_first,
            merge_close_events,
            ignore_borders,
            ignore_channels: ignore_channels.into(),
        }
    }
}

use std::{ops::Range, path::PathBuf};

use egui::Visuals;
use egui_plot::{Legend, PlotUi};

use numass_processing::{
    postprocess::{PostProcessParams, post_process_frame},
    preprocess::{
        PreprocessParams, ProcessedPoint, apply_waveforms, draw_processed_frame, draw_raw_frame,
        extract_waveforms,
    },
    process::{ProcessParams, convert_to_kev, extract_frame},
    storage::load_point,
    types::{FrameEvent, NumassWaveformsFast},
    widgets::{HISTOGRAM_MAX, HISTOGRAM_MIN},
};

use numass_processing::widgets::UserInput;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    fn download(filename: &str, text: &str);
}

pub struct FilteredViewer<'a> {
    preprocess_params: PreprocessParams,
    process: ProcessParams,
    postprocess: PostProcessParams,
    range: Range<f32>,
    waveforms: NumassWaveformsFast<'a>,
    processed: ProcessedPoint,
    raw_hidden_seeded: bool,
    indexes: Option<Vec<u64>>,
    current: usize,
}

impl<'a> FilteredViewer<'a> {
    pub async fn init_with_point(
        filepath: PathBuf,
        preprocess_params: PreprocessParams,
        process: ProcessParams,
        postprocess: PostProcessParams,
        range: Range<f32>,
    ) -> Self {
        let point = load_point(&filepath).await;
        let point = Box::leak::<'a>(Box::new(point)); // TODO: set lifetime properly
        let waveforms = extract_waveforms(point);
        let processed = apply_waveforms(&waveforms, &preprocess_params.steps);

        let mut viewer = Self {
            preprocess_params,
            process,
            postprocess,
            range,
            waveforms,
            processed,
            raw_hidden_seeded: false,
            indexes: None,
            current: 0,
        };

        viewer.update_indexes();
        viewer
    }

    fn reprocess(&mut self) {
        self.processed = apply_waveforms(&self.waveforms, &self.preprocess_params.steps);
        self.update_indexes();
    }

    fn update_indexes(&mut self) {
        self.current = 0;

        let mut new_indexes = vec![];

        for (time, frame) in &self.waveforms {
            let events = post_process_frame(
                extract_frame(
                    &self.processed[time],
                    frame,
                    &self.process.algorithm,
                    &mut None,
                ),
                &self.postprocess,
                None,
            );

            for (_, event) in events {
                if let FrameEvent::Event {
                    channel, amplitude, ..
                } = event
                {
                    let amplitude = if self.process.convert_to_kev {
                        convert_to_kev(&amplitude, channel, &self.process.algorithm)
                    } else {
                        amplitude
                    };
                    if self.range.contains(&amplitude) {
                        new_indexes.push(*time);
                        break;
                    }
                }
            }
        }

        self.indexes = Some(new_indexes);
    }

    fn plot_processed_frame(&self, plot_ui: &mut PlotUi<'_>, time: u64) {
        let frame = self.waveforms.get(&time).unwrap();
        let processed = &self.processed[&time];

        draw_raw_frame(frame, plot_ui);
        draw_processed_frame(processed, plot_ui);

        let mut events = extract_frame(
            processed,
            frame,
            &self.process.algorithm,
            &mut Some(plot_ui),
        );
        events = post_process_frame(events, &self.postprocess, Some(plot_ui));

        if self.process.convert_to_kev {
            events.iter_mut().for_each(|(_, event)| {
                if let FrameEvent::Event {
                    channel: ch_id,
                    amplitude,
                    ..
                } = event
                {
                    *amplitude = convert_to_kev(amplitude, *ch_id, &self.process.algorithm)
                }
            });
        }
    }

    fn inc(&mut self) {
        if let Some(len) = self.indexes.as_ref().map(|indexes| indexes.len())
            && len > 0
            && self.current + 1 < len
        {
            self.current += 1
        }
    }

    fn dec(&mut self) {
        if self.indexes.is_some() && self.current != 0 {
            self.current -= 1
        }
    }
}

impl eframe::App for FilteredViewer<'_> {
    #[allow(unused_variables)]
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        ctx.set_visuals(Visuals::dark());

        let indexes_len = self.indexes.as_ref().map(|indexes| indexes.len());

        ctx.input(|i| {
            if i.key_pressed(eframe::egui::Key::ArrowLeft) {
                self.dec()
            }
            if i.key_pressed(eframe::egui::Key::ArrowRight) {
                self.inc()
            }
        });

        eframe::egui::SidePanel::left("parameters").show(ctx, |ui| {
            eframe::egui::TopBottomPanel::bottom("version").show_inside(ui, |ui| {
                crate::version_label(ui);
            });

            self.preprocess_params = self.preprocess_params.input(ui, ctx);

            ui.separator();

            self.process = self.process.input(ui, ctx);

            ui.separator();

            self.postprocess = self.postprocess.input(ui, ctx);

            ui.separator();
            let mut min = self.range.start;
            ui.add(egui::Slider::new(&mut min, HISTOGRAM_MIN..=HISTOGRAM_MAX).text("left"));
            let mut max = self.range.end;
            ui.add(egui::Slider::new(&mut max, HISTOGRAM_MIN..=HISTOGRAM_MAX).text("right"));
            self.range = min..max;

            if ui.button("apply").clicked() {
                self.reprocess();
            }
        });

        if self
            .indexes
            .as_ref()
            .is_none_or(|indexes| indexes.is_empty())
        {
            eframe::egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label("No data to display");
                });
            });
        } else {
            eframe::egui::TopBottomPanel::top("position").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    let width = {
                        let mut x = 0.0;
                        ctx.input(|i| x = i.screen_rect().width());
                        x
                    };
                    #[cfg(target_arch = "wasm32")]
                    let width = eframe::web_sys::window()
                        .unwrap()
                        .inner_width()
                        .unwrap()
                        .as_f64()
                        .unwrap() as f32;

                    ui.style_mut().spacing.slider_width = width - 450.0;

                    if let Some(len) = indexes_len {
                        ui.add(
                            eframe::egui::Slider::new(&mut self.current, 0..=len - 1).step_by(1.0),
                        );
                        if ui.button("<").clicked() {
                            self.dec();
                        }
                        if ui.button(">").clicked() {
                            self.inc();
                        }
                    }

                    if let Some(indexes) = self.indexes.as_ref() {
                        ui.label(format!("{:.3} ms", indexes[self.current] as f64 / 1e6));
                    }
                })
            });

            eframe::egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(indexes) = self.indexes.as_ref() {
                    let legend = if self.raw_hidden_seeded {
                        Legend::default()
                    } else {
                        self.raw_hidden_seeded = true;
                        Legend::default()
                            .hidden_items((0..7).map(|ch| egui::Id::new(format!("ch #{}", ch + 1))))
                    };

                    egui_plot::Plot::new("waveforms")
                        .legend(legend)
                        .x_axis_formatter(|mark, _| {
                            format!("{:.3} μs", (mark.value * 8.0) / 1000.0)
                        })
                        .show(ui, |plot_ui| {
                            if indexes.is_empty() {
                                return;
                            }

                            let position = indexes[self.current];
                            self.plot_processed_frame(plot_ui, position);
                        });
                } else {
                    ui.spinner();
                }
            });
        }
    }
}

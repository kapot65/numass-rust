use std::{collections::BTreeMap, ops::Range, path::PathBuf};

use egui::Visuals;
use egui_plot::{Legend, PlotUi};

use numass_processing::{
    numass::protos::rsb_event,
    postprocess::{PostProcessParams, post_process_frame},
    preprocess::{
        PreprocessParams, ProcessedPoint, apply_waveforms, draw_processed_frame, draw_raw_frame,
        extract_waveforms, frame_to_waveform,
    },
    process::{ProcessParams, convert_to_kev, extract_frame},
    storage::load_point,
    types::{FrameEvent, NumassFrameFast, NumassWaveformsFast, nanoseconds},
    utils::{correct_frame_time, format_time},
    widgets::{HISTOGRAM_MAX, HISTOGRAM_MIN, UserInput},
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    fn download(filename: &str, text: &str);
}

pub struct FilteredViewer {
    preprocess_params: PreprocessParams,
    process: ProcessParams,
    postprocess: PostProcessParams,
    range: Range<f32>,
    /// Точка принадлежит вьюеру: вейформы заимствуются из неё на время
    /// операций (вместо утечки через `Box::leak`).
    point: rsb_event::Point,
    processed: ProcessedPoint,
    raw_hidden_seeded: bool,
    indexes: Option<Vec<u64>>,
    current: usize,
    /// Ошибка пайплайна (скрипт/данные): рисуется банером вместо графика,
    /// снимается успешным reprocess.
    error: Option<String>,
}

impl FilteredViewer {
    pub async fn init_with_point(
        filepath: PathBuf,
        preprocess_params: PreprocessParams,
        process: ProcessParams,
        postprocess: PostProcessParams,
        range: Range<f32>,
    ) -> Self {
        // Битый файл — не паника: вьюер открывается с банером ошибки.
        let point = match load_point(&filepath).await {
            Ok(point) => point,
            Err(err) => {
                return Self {
                    preprocess_params,
                    process,
                    postprocess,
                    range,
                    point: rsb_event::Point::default(),
                    processed: ProcessedPoint::default(),
                    raw_hidden_seeded: false,
                    indexes: None,
                    current: 0,
                    error: Some(err.to_string()),
                };
            }
        };
        let waveforms = extract_waveforms(&point);

        let (processed, indexes, error) =
            match apply_waveforms(&waveforms, &preprocess_params.steps) {
                Ok(processed) => {
                    match Self::matching_frames(
                        &processed,
                        &process,
                        &postprocess,
                        &range,
                        &waveforms,
                    ) {
                        Ok(indexes) => (processed, Some(indexes), None),
                        Err(err) => (processed, None, Some(err)),
                    }
                }
                Err(err) => (ProcessedPoint::default(), None, Some(err.to_string())),
            };

        Self {
            preprocess_params,
            process,
            postprocess,
            range,
            point,
            processed,
            raw_hidden_seeded: false,
            indexes,
            current: 0,
            error,
        }
    }

    fn reprocess(&mut self) {
        let waveforms = extract_waveforms(&self.point);

        let (processed, indexes, error) =
            match apply_waveforms(&waveforms, &self.preprocess_params.steps) {
                Ok(processed) => match Self::matching_frames(
                    &processed,
                    &self.process,
                    &self.postprocess,
                    &self.range,
                    &waveforms,
                ) {
                    Ok(indexes) => (processed, Some(indexes), None),
                    Err(err) => (processed, None, Some(err)),
                },
                Err(err) => (ProcessedPoint::default(), None, Some(err.to_string())),
            };

        self.processed = processed;
        self.indexes = indexes;
        self.current = 0;
        self.error = error;
    }

    /// Времена кадров, в которых есть хотя бы одно событие с амплитудой в `range`.
    fn matching_frames(
        processed: &ProcessedPoint,
        process: &ProcessParams,
        postprocess: &PostProcessParams,
        range: &Range<f32>,
        waveforms: &NumassWaveformsFast<'_>,
    ) -> Result<Vec<u64>, String> {
        let mut indexes = vec![];

        for (time, frame) in waveforms {
            let events = extract_frame(&processed[time], frame, &process.algorithm, &mut None)
                .and_then(|events| post_process_frame(events, postprocess, None))
                .map_err(|err| err.to_string())?;

            for (_, event) in events {
                if let FrameEvent::Event {
                    channel, amplitude, ..
                } = event
                {
                    let amplitude = if process.convert_to_kev {
                        convert_to_kev(&amplitude, channel, &process.algorithm)
                            .map_err(|err| err.to_string())?
                    } else {
                        amplitude
                    };
                    if range.contains(&amplitude) {
                        indexes.push(*time);
                        break;
                    }
                }
            }
        }

        Ok(indexes)
    }

    /// Сырые вейвформы каналов кадра с данным временем (без копирования).
    fn frame_at(&self, time: u64) -> NumassFrameFast<'_> {
        let mut frame = BTreeMap::new();

        for channel in &self.point.channels {
            for block in &channel.blocks {
                for point_frame in &block.frames {
                    if correct_frame_time(point_frame.time) == time {
                        frame.insert(channel.id as u8, frame_to_waveform(point_frame));
                    }
                }
            }
        }

        frame
    }

    /// Отрисовка кадра + событий. Ошибка пайплайна (скрипт/данные) возвращается
    /// текстом: `ui()` показывает её банером вместо паники.
    fn plot_processed_frame(&self, plot_ui: &mut PlotUi<'_>, time: u64) -> Option<String> {
        let frame = self.frame_at(time);
        let processed = &self.processed[&time];

        draw_raw_frame(&frame, plot_ui);
        draw_processed_frame(processed, plot_ui);

        let pipeline_result = extract_frame(
            processed,
            &frame,
            &self.process.algorithm,
            &mut Some(plot_ui),
        )
        .and_then(|events| post_process_frame(events, &self.postprocess, Some(plot_ui)));
        let mut events = match pipeline_result {
            Ok(events) => events,
            Err(err) => return Some(err.to_string()),
        };

        let mut kev_error = None;
        if self.process.convert_to_kev {
            events.iter_mut().for_each(|(_, event)| {
                if kev_error.is_some() {
                    return;
                }
                if let FrameEvent::Event {
                    channel: ch_id,
                    amplitude,
                    ..
                } = event
                {
                    *amplitude = match convert_to_kev(amplitude, *ch_id, &self.process.algorithm) {
                        Ok(value) => value,
                        Err(err) => {
                            kev_error = Some(err.to_string());
                            return;
                        }
                    };
                }
            });
        }
        kev_error
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

impl eframe::App for FilteredViewer {
    #[allow(unused_variables)]
    fn ui(&mut self, ui: &mut eframe::egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
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

        eframe::egui::Panel::left("parameters").show(ui, |ui| {
            eframe::egui::Panel::bottom("version").show(ui, |ui| {
                crate::version_label(ui);
            });

            self.preprocess_params = self.preprocess_params.input(ui, &ctx);

            ui.separator();

            self.process = self.process.input(ui, &ctx);

            ui.separator();

            self.postprocess = self.postprocess.input(ui, &ctx);

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

        // Ошибка пайплайна — верхняя панель: видна и в «No data»,
        // и в рисовании графика.
        if let Some(error) = &self.error {
            eframe::egui::Panel::top("pipeline-error").show(ui, |ui| {
                ui.label(
                    egui::RichText::new(error)
                        .small()
                        .monospace()
                        .color(ui.visuals().error_fg_color),
                );
            });
        }

        if self
            .indexes
            .as_ref()
            .is_none_or(|indexes| indexes.is_empty())
        {
            eframe::egui::CentralPanel::default().show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label("No data to display");
                });
            });
        } else {
            eframe::egui::Panel::top("position").show(ui, |ui| {
                ui.horizontal(|ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    let width = ctx.viewport_rect().width();
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
                        ui.label(format_time(nanoseconds(indexes[self.current] as f64)));
                    }
                })
            });

            eframe::egui::CentralPanel::default().show(ui, |ui| {
                let mut plot_error = None;
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
                        .x_axis_label("Time")
                        .y_axis_label("Amplitude")
                        .x_axis_formatter(|mark, _| format_time(nanoseconds(mark.value)))
                        .show(ui, |plot_ui| {
                            if indexes.is_empty() {
                                return;
                            }

                            let position = indexes[self.current];
                            plot_error = self.plot_processed_frame(plot_ui, position);
                        });
                } else {
                    ui.spinner();
                }
                if let Some(error) = plot_error {
                    self.error = Some(error);
                }
            });
        }
    }
}

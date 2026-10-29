use std::{collections::BTreeMap, mem, ops::Range, path::PathBuf, sync::Arc};

use egui::{Visuals, mutex::Mutex};
use egui_plot::{Legend, PlotUi};

use numass_processing::{
    numass::protos::rsb_event,
    postprocess::{PostProcessParams, post_process_frame},
    preprocess::{
        PreprocessParams, PreprocessStep, ProcessedPoint, draw_processed_frame, draw_raw_frame,
        extract_waveforms, frame_to_waveform,
    },
    process::{ProcessParams, convert_to_kev, extract_frame},
    storage::load_point,
    types::{FrameEvent, NumassFrameFast, NumassWaveformsFast, nanoseconds},
    utils::{correct_frame_time, format_time, waveform_hover_label},
    widgets::{HISTOGRAM_MAX, HISTOGRAM_MIN, UserInput},
};

#[cfg(not(target_arch = "wasm32"))]
use tokio::spawn;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local as spawn;

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
    /// операций (вместо утечки через `Box::leak`). Аc — чтобы задача
    /// репроцесса (wasm: это worker, выполняющий Lua) читала точку без копии.
    point: Arc<rsb_event::Point>,
    /// Результат предобработки; пишется задачей репроцесса, читается ui.
    processed: Arc<Mutex<ProcessedPoint>>,
    raw_hidden_seeded: bool,
    indexes: Arc<Mutex<Option<Vec<u64>>>>,
    current: usize,
    /// Ошибка пайплайна (скрипт/данные): рисуется банером вместо графика,
    /// снимается успешным репроцессом.
    error: Arc<Mutex<Option<String>>>,
    /// Пока идёт репроцесс, повторные клики по apply игнорируются.
    reprocess_running: Arc<Mutex<bool>>,
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
            Ok(point) => Arc::new(point),
            Err(err) => {
                return Self {
                    preprocess_params,
                    process,
                    postprocess,
                    range,
                    point: Arc::new(rsb_event::Point::default()),
                    processed: Arc::new(Mutex::new(ProcessedPoint::default())),
                    raw_hidden_seeded: false,
                    indexes: Arc::new(Mutex::new(None)),
                    current: 0,
                    error: Arc::new(Mutex::new(Some(err.to_string()))),
                    reprocess_running: Arc::new(Mutex::new(false)),
                };
            }
        };

        let viewer = Self {
            preprocess_params,
            process,
            postprocess,
            range,
            point,
            processed: Arc::new(Mutex::new(ProcessedPoint::default())),
            raw_hidden_seeded: false,
            indexes: Arc::new(Mutex::new(None)),
            current: 0,
            error: Arc::new(Mutex::new(None)),
            reprocess_running: Arc::new(Mutex::new(false)),
        };

        // Первый прогон здесь и синхронизован await'ом: вьюер появится после
        // полной загрузки, одинаково на обеих платформах.
        FilteredViewer::reprocess_task(
            Arc::clone(&viewer.point),
            viewer.preprocess_params.steps.clone(),
            viewer.process.clone(),
            viewer.postprocess,
            viewer.range.clone(),
            Arc::clone(&viewer.processed),
            Arc::clone(&viewer.indexes),
            Arc::clone(&viewer.error),
            Arc::clone(&viewer.reprocess_running),
            None,
        )
        .await;

        viewer
    }

    /// Кнопка apply: запуск репроцесса в фоне.
    fn reprocess(&mut self, ctx: eframe::egui::Context) {
        {
            let mut running = self.reprocess_running.lock();
            if *running {
                return;
            }
            *running = true;
        }

        spawn(Self::reprocess_task(
            Arc::clone(&self.point),
            self.preprocess_params.steps.clone(),
            self.process.clone(),
            self.postprocess,
            self.range.clone(),
            Arc::clone(&self.processed),
            Arc::clone(&self.indexes),
            Arc::clone(&self.error),
            Arc::clone(&self.reprocess_running),
            Some(ctx),
        ));
    }

    /// Прогон пайплайна на переданных параметрах со записью в state вьюера.
    ///
    /// Одинаковый на обеих платформах: разница только в том, где исполняет
    /// скриптовый шаг `script_worker::apply_waveforms_with` (native — локально,
    /// wasm — отдельный worker-модуль, Lua-throw не убивает интерфейс).
    #[allow(clippy::too_many_arguments)]
    async fn reprocess_task(
        point: Arc<rsb_event::Point>,
        steps: Vec<PreprocessStep>,
        process: ProcessParams,
        postprocess: PostProcessParams,
        range: Range<f32>,
        processed_slot: Arc<Mutex<ProcessedPoint>>,
        indexes_slot: Arc<Mutex<Option<Vec<u64>>>>,
        error_slot: Arc<Mutex<Option<String>>>,
        running_slot: Arc<Mutex<bool>>,
        repaint: Option<eframe::egui::Context>,
    ) {
        // Старая копия уходит до нового просчёта. Иначе на wasm пик
        // (старый ProcessedPoint + новый) навсегда растит кучу: memory.grow
        // страницы не отдаёт. Индекс сбрасывается вместе с ней, чтобы ui
        // не рисовал кадр по ключу, которого уже нет.
        drop(mem::take(&mut *processed_slot.lock()));
        *indexes_slot.lock() = None;

        let waveforms = extract_waveforms(&point);

        let (processed, indexes, error) =
            match crate::script_worker::apply_waveforms_with(&waveforms, &steps, None).await {
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

        *processed_slot.lock() = processed;
        *indexes_slot.lock() = indexes;
        *error_slot.lock() = error;
        *running_slot.lock() = false;

        if let Some(ctx) = repaint {
            ctx.request_repaint();
        }
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
        let processed = self.processed.lock();
        // Репроцесс забирает слот до нового расчёта: индекс мог ещё указывать
        // на кадр, которого в карте уже нет.
        let Some(processed) = processed.get(&time) else {
            return None;
        };

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
        if let Some(len) = self.indexes.lock().as_ref().map(|indexes| indexes.len())
            && len > 0
            && self.current + 1 < len
        {
            self.current += 1
        }
    }

    fn dec(&mut self) {
        if self.indexes.lock().is_some() && self.current != 0 {
            self.current -= 1
        }
    }
}

impl eframe::App for FilteredViewer {
    #[allow(unused_variables)]
    fn ui(&mut self, ui: &mut eframe::egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.set_visuals(Visuals::dark());

        let indexes_len = self.indexes.lock().as_ref().map(|indexes| indexes.len());
        let reprocess_running = *self.reprocess_running.lock();
        let mut slider_resp: Option<egui::Response> = None;

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

            let apply_label = if reprocess_running {
                "processing..."
            } else {
                "apply"
            };
            if ui.button(apply_label).clicked() {
                self.reprocess(ctx.clone());
            }
        });

        // Ошибка пайплайна — верхняя панель: видна и в «No data»,
        // и при рисовании графика.
        if let Some(error) = self.error.lock().as_ref() {
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
            .lock()
            .as_ref()
            .is_none_or(|indexes| indexes.is_empty())
        {
            eframe::egui::CentralPanel::default().show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    if reprocess_running {
                        ui.spinner();
                    } else {
                        ui.label("No data to display");
                    }
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
                        slider_resp = Some(ui.add(
                            eframe::egui::Slider::new(&mut self.current, 0..=len - 1).step_by(1.0),
                        ));
                        if ui.button("<").clicked() {
                            self.dec();
                        }
                        if ui.button(">").clicked() {
                            self.inc();
                        }
                    }

                    if let Some(indexes) = self.indexes.lock().as_ref() {
                        ui.label(format_time(nanoseconds(indexes[self.current] as f64)));
                    }
                })
            });

            eframe::egui::CentralPanel::default().show(ui, |ui| {
                let mut plot_error = None;
                let plot_response = if let Some(indexes) = self.indexes.lock().as_ref() {
                    let legend = if self.raw_hidden_seeded {
                        Legend::default()
                    } else {
                        self.raw_hidden_seeded = true;
                        Legend::default()
                            .hidden_items((0..7).map(|ch| egui::Id::new(format!("ch #{}", ch + 1))))
                    };

                    Some(
                        egui_plot::Plot::new("waveforms")
                            .legend(legend)
                            .x_axis_label("Time")
                            .y_axis_label("Amplitude")
                            .x_axis_formatter(|mark, _| format_time(nanoseconds(mark.value)))
                            .label_formatter(waveform_hover_label)
                            .show(ui, |plot_ui| {
                                if indexes.is_empty() {
                                    return;
                                }

                                let position = indexes[self.current];
                                plot_error = self.plot_processed_frame(plot_ui, position);
                            }),
                    )
                } else {
                    ui.spinner();
                    None
                };
                if let Some(error) = plot_error {
                    *self.error.lock() = Some(error);
                }

                // Стрелки листания — только при наведении на слайдер позиции
                // или график (см. crate::arrow_nav_on_hover).
                if let (Some(slider), Some(response)) =
                    (slider_resp.as_ref(), plot_response.as_ref())
                    && let Some(dir) = crate::arrow_nav_on_hover(ui, slider, &response.response)
                {
                    match dir {
                        crate::ArrowNav::Prev => self.dec(),
                        crate::ArrowNav::Next => self.inc(),
                    }
                }
            });
        }
    }
}

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use crate::ChunkedData;
use egui::{Visuals, mutex::Mutex};
use egui_plot::{Legend, Points};
use numass_processing::{
    numass::{NumassMeta, protos::rsb_event},
    postprocess::{PostProcessParams, post_process},
    preprocess::PreprocessParams,
    process::{ProcessParams, extract_events},
    storage::{load_meta, load_point},
    types::{FrameEvent, Time, millisecond, nanosecond, nanoseconds, time_ns},
    utils::{acquisition_time, color_for_index, format_time, repair_frame_times, timestamps_bound},
    widgets::UserInput,
};

#[cfg(not(target_arch = "wasm32"))]
use tokio::spawn;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local as spawn;

type Chunk = Vec<(u8, Time, f32)>;

pub struct BundleViewer {
    point: Arc<Mutex<Option<rsb_event::Point>>>, // TODO: redownload point instead of storing?
    meta: Arc<Mutex<Option<NumassMeta>>>,        // TODO: redownload meta instead of storing?

    preprocess: PreprocessParams,
    process: ProcessParams,
    post_process: PostProcessParams,
    limit_ms: u64,

    data: Arc<Mutex<Option<ChunkedData<Chunk>>>>,
    current_chunk: usize,
}

/// Раскладывает события по чанкам длиной `limit`.
///
/// Времена кадров (ключи событий, `BTreeMap` — по возрастанию) проходят
/// тот же ремонт, что и кадры point-viewer: [`repair_frame_times`] ставит
/// сегменты сбоя следом за валидной шкалой вместо отбрасывания. События
/// кадров с невосстановимым временем не рисуются и попадают в `dropped`.
/// Без `acquisition_time` (нет мета) границы и ремонта нет.
fn point_to_chunks(
    meta: Option<NumassMeta>,
    point: rsb_event::Point,
    preprocess: PreprocessParams,
    process: ProcessParams,
    postprocess: PostProcessParams,
    limit: Time,
) -> ChunkedData<Chunk> {
    let limit_ns = time_ns(limit);
    let bound_ns = acquisition_time(meta.clone()).map(timestamps_bound);
    let (events, _) = post_process(
        extract_events(meta, point, &preprocess, &process),
        &postprocess,
    );

    let frame_times: Vec<u64> = events.keys().copied().collect();
    let frame_times = match bound_ns {
        Some(bound) => repair_frame_times(&frame_times, bound),
        // без метаданных времена не проверяем — как раньше
        None => frame_times.iter().map(|&time| Some(time)).collect(),
    };

    let mut chunks = vec![vec![]];
    let mut dropped = 0;

    for ((_, timed_event), frame_time) in events.into_iter().zip(frame_times) {
        let Some(frame_time) = frame_time else {
            // Время кадра невосстановимо: его события не рисуем.
            dropped += timed_event
                .iter()
                .filter(|(_, event)| matches!(event, FrameEvent::Event { .. }))
                .count();
            continue;
        };

        for (offset, event) in timed_event {
            if let FrameEvent::Event {
                channel, amplitude, ..
            } = event
            {
                let time = frame_time + offset as u64;
                let chunk_num = (time / limit_ns) as usize;

                while chunks.len() < chunk_num + 1 {
                    chunks.push(vec![])
                }

                chunks[chunk_num].push((channel, nanoseconds((time % limit_ns) as f64), amplitude));
            }
        }
    }

    ChunkedData { chunks, dropped }
}

impl BundleViewer {
    pub fn init_with_point(
        filepath: PathBuf,
        preprocess: PreprocessParams,
        process: ProcessParams,
        post_process: PostProcessParams,
    ) -> Self {
        let viewer = BundleViewer {
            point: Arc::new(Mutex::new(None)),
            meta: Arc::new(Mutex::new(None)),
            preprocess,
            process,
            post_process,
            limit_ms: 100,
            data: Arc::new(Mutex::new(None)),
            current_chunk: 0,
        };

        let point = Arc::clone(&viewer.point);
        let meta = Arc::clone(&viewer.meta);
        let data = Arc::clone(&viewer.data);
        let limit = Time::new::<millisecond>(viewer.limit_ms as f64);
        let process = viewer.process.to_owned();
        let preprocess = viewer.preprocess.to_owned();
        let post_process = viewer.post_process.to_owned();

        spawn(async move {
            let point_local = load_point(&filepath).await;
            *point.lock() = Some(point_local);

            let meta_local = load_meta(&filepath).await;
            *meta.lock() = meta_local;

            BundleViewer::recalculate_chunks(
                meta,
                point,
                data,
                preprocess,
                process,
                post_process,
                limit,
            );
        });

        viewer
    }

    fn recalculate_chunks(
        meta: Arc<Mutex<Option<NumassMeta>>>,
        point: Arc<Mutex<Option<rsb_event::Point>>>,
        data: Arc<Mutex<Option<ChunkedData<Chunk>>>>,
        preprocess: PreprocessParams,
        process: ProcessParams,
        post_process: PostProcessParams,
        limit: Time,
    ) {
        *data.lock() = None;

        if let Some(point) = &*point.lock() {
            let data_local = Some(point_to_chunks(
                meta.lock().clone(),
                point.clone(),
                preprocess,
                process,
                post_process,
                limit,
            ));
            *data.lock() = data_local;
        }
    }
}

// TODO: add visualization for resets, overflows
impl eframe::App for BundleViewer {
    #[allow(unused_variables)]
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.set_visuals(Visuals::dark());

        if let Some(data) = &*self.data.lock() {
            let chunks = &data.chunks;
            ctx.input(|i| {
                if i.key_pressed(eframe::egui::Key::ArrowRight)
                    && self.current_chunk < chunks.len() - 1
                {
                    self.current_chunk += 1;
                }
                if i.key_pressed(eframe::egui::Key::ArrowLeft) && self.current_chunk > 0 {
                    self.current_chunk -= 1;
                }
            });
        }

        eframe::egui::SidePanel::left("parameters").show(ctx, |ui| {
            eframe::egui::TopBottomPanel::bottom("version").show_inside(ui, |ui| {
                crate::version_label(ui);
            });

            self.preprocess = self.preprocess.input(ui, ctx);

            ui.separator();

            self.process = self.process.input(ui, ctx);

            ui.separator();

            self.post_process = self.post_process.input(ui, ctx);

            ui.separator();

            ui.add(egui::Slider::new(&mut self.limit_ms, 1..=1000).text("bin size (ms)"));

            if ui.button("apply").clicked() {
                self.current_chunk = 0; // Reset to the first chunk when applying changes.

                let meta = Arc::clone(&self.meta);
                let point = Arc::clone(&self.point);
                let data = Arc::clone(&self.data);
                let limit = Time::new::<millisecond>(self.limit_ms as f64);
                let process = self.process.to_owned();
                let preprocess = self.preprocess.to_owned();
                let post_process = self.post_process.to_owned();

                spawn(async move {
                    BundleViewer::recalculate_chunks(
                        meta,
                        point,
                        data,
                        preprocess,
                        process,
                        post_process,
                        limit,
                    );
                });
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(data) = &*self.data.lock() {
                let chunks = &data.chunks;
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

                ui.horizontal(|ui| {
                    // Слайдер листает чанки: показываем время начала чанка, а не его номер.
                    let chunk_len = Time::new::<millisecond>(self.limit_ms as f64);
                    ui.add(
                        egui::Slider::new(&mut self.current_chunk, 0..=chunks.len() - 1)
                            .custom_formatter(move |value, _| format_time(chunk_len * value))
                            .step_by(1.0),
                    );
                    if ui.button("<").clicked() && self.current_chunk > 0 {
                        self.current_chunk -= 1;
                    }
                    if ui.button(">").clicked() && self.current_chunk < chunks.len() - 1 {
                        self.current_chunk += 1;
                    }
                });

                // События кадров с невосстановимым временем не рисуются —
                // показываем, сколько их отброшено.
                ui.weak(format!(
                    "{} events dropped: unrecoverable timestamps",
                    data.dropped
                ));

                let amplitude_label = if self.process.convert_to_kev {
                    "Amplitude, keV"
                } else {
                    "Amplitude, ADC"
                };

                egui_plot::Plot::new("waveforms")
                    .legend(Legend::default())
                    .x_axis_label("Time")
                    .y_axis_label(amplitude_label)
                    .x_axis_formatter(|mark, _| format_time(nanoseconds(mark.value)))
                    .show(ui, |plot_ui| {
                        let mut channel_points = BTreeMap::new();

                        for (ch_num, offset, amp) in chunks[self.current_chunk].clone() {
                            channel_points
                                .entry(ch_num)
                                .or_insert(vec![])
                                .push([offset.get::<nanosecond>(), amp as f64]);
                        }

                        for (ch_num, points) in channel_points {
                            plot_ui.points(
                                Points::new(format!("ch #{}", ch_num + 1), points)
                                    .color(color_for_index((ch_num) as usize))
                                    .radius(3.0_f32),
                            )
                        }
                    });
            } else {
                ui.spinner();
            }
        });
    }
}

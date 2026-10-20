use std::{path::PathBuf, sync::Arc};

use crate::ChunkedData;
use egui::{Visuals, mutex::Mutex};
use egui_plot::{GridMark, Legend};
use numass_processing::{
    numass::protos::rsb_event,
    storage::{load_meta, load_point},
    types::{ProcessedWaveform, RawWaveform, Time, millisecond, nanoseconds, time_ns},
    utils::{
        EguiLine, acquisition_time, color_for_index, correct_frame_time, format_time,
        repair_frame_times, timestamps_bound,
    },
};

#[cfg(not(target_arch = "wasm32"))]
use tokio::spawn;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local as spawn;

#[derive(Debug, Clone)]
enum AppState {
    Initializing,
    FirstLoad,
    Interactive,
}

type Chunk = Vec<(u8, Time, ProcessedWaveform)>;

pub struct PointViewer {
    data: Arc<Mutex<Option<ChunkedData<Chunk>>>>,
    current_chunk: usize,
    state: Arc<Mutex<AppState>>,
}

/// Кадры точки с исправленными таймстампами: (время, канал, кадр).
fn frames_with_times(
    point: &rsb_event::Point,
) -> Vec<(u64, u8, &rsb_event::point::channel::block::Frame)> {
    let mut frames = vec![];

    for channel in &point.channels {
        for block in &channel.blocks {
            for frame in &block.frames {
                frames.push((correct_frame_time(frame.time), channel.id as u8, frame));
            }
        }
    }

    frames
}

/// Раскладывает кадры по чанкам длиной `limit`.
///
/// Кадры с мусорным таймстампом не теряются: [`repair_frame_times`]
/// «ремонтирует» их по модели сбоя из [`correct_frame_time`] (константа,
/// примешанная начиная с определенного триггера) и ставит следом за
/// валидной шкалой. Невосстановимые кадры отбрасываются и попадают в
/// `dropped`. Без `acquisition_time` границы и ремонта нет.
fn point_to_chunks(
    point: rsb_event::Point,
    limit: Time,
    acquisition: Option<Time>,
) -> ChunkedData<Chunk> {
    let limit_ns = time_ns(limit);
    let frames = frames_with_times(&point);
    let times: Vec<u64> = frames.iter().map(|(time, ..)| *time).collect();

    let times = match acquisition.map(timestamps_bound) {
        Some(bound) => repair_frame_times(&times, bound),
        // без метаданных времена не проверяем — как раньше
        None => times.iter().map(|&time| Some(time)).collect(),
    };

    let mut chunks = vec![vec![]];
    let mut dropped = 0;

    for ((_, channel, frame), time) in frames.into_iter().zip(times) {
        let Some(time) = time else {
            dropped += 1;
            continue;
        };

        let chunk_num = (time / limit_ns) as usize;
        while chunks.len() < chunk_num + 1 {
            chunks.push(vec![])
        }

        chunks[chunk_num].push((
            channel,
            nanoseconds((time % limit_ns) as f64),
            ProcessedWaveform::from(&RawWaveform::from(frame)),
        ));
    }

    ChunkedData { chunks, dropped }
}

/// Ближайший непустой чанк перед `current` (не включая его).
/// Если непустых чанков перед ним нет, возвращает `current`.
fn prev_non_empty_chunk(chunks: &[Chunk], current: usize) -> usize {
    chunks[..current]
        .iter()
        .rposition(|chunk| !chunk.is_empty())
        .unwrap_or(current)
}

/// Ближайший непустой чанк после `current` (не включая его).
/// Если непустых чанков после него нет, возвращает `current`.
fn next_non_empty_chunk(chunks: &[Chunk], current: usize) -> usize {
    chunks[current + 1..]
        .iter()
        .position(|chunk| !chunk.is_empty())
        .map_or(current, |index| index + current + 1)
}

impl PointViewer {
    pub fn init_with_point(filepath: PathBuf) -> Self {
        let viewer = PointViewer {
            data: Arc::new(Mutex::new(None)),
            current_chunk: 0,
            state: Arc::new(Mutex::new(AppState::Initializing)),
        };

        let data = Arc::clone(&viewer.data);
        let state = Arc::clone(&viewer.state);

        spawn(async move {
            let point = load_point(&filepath).await;
            let acquisition = acquisition_time(load_meta(&filepath).await);
            *data.lock() = Some(point_to_chunks(
                point,
                Time::new::<millisecond>(1.0),
                acquisition,
            ));
            *state.lock() = AppState::FirstLoad;
        });

        viewer
    }
}

impl eframe::App for PointViewer {
    #[allow(unused_variables)]
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.set_visuals(Visuals::dark());

        let state = self.state.lock().clone();

        match state {
            AppState::Initializing => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.spinner();
                });
            }
            AppState::FirstLoad => {
                *self.state.lock() = AppState::Interactive;
            }
            AppState::Interactive => {
                if let Some(data) = self.data.lock().as_ref() {
                    let chunks = &data.chunks;
                    // consume_key, чтобы сфокусированный (через Tab) слайдер
                    // не обрабатывал стрелки повторно.
                    ctx.input_mut(|i| {
                        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
                            self.current_chunk = next_non_empty_chunk(chunks, self.current_chunk);
                        }
                        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                            self.current_chunk = prev_non_empty_chunk(chunks, self.current_chunk);
                        }
                    });

                    egui::TopBottomPanel::bottom("version").show(ctx, |ui| {
                        crate::version_label(ui);
                    });

                    egui::CentralPanel::default().show(ctx, |ui| {
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

                        ui.style_mut().spacing.slider_width = width - 150.0;

                        ui.horizontal(|ui| {
                            // чанки по 1 мс: показываем время начала чанка, а не номер
                            ui.add(
                                egui::Slider::new(&mut self.current_chunk, 0..=chunks.len() - 1)
                                    .custom_formatter(|value, _| {
                                        format_time(Time::new::<millisecond>(value))
                                    })
                                    .step_by(1.0),
                            );
                            if ui.button("<").clicked() {
                                self.current_chunk =
                                    prev_non_empty_chunk(chunks, self.current_chunk);
                            }
                            if ui.button(">").clicked() {
                                self.current_chunk =
                                    next_non_empty_chunk(chunks, self.current_chunk);
                            }
                        });

                        // Кадры с невосстановимым временем не рисуются —
                        // показываем, сколько их отброшено.
                        ui.weak(format!(
                            "{} frames dropped: unrecoverable timestamps",
                            data.dropped
                        ));

                        egui_plot::Plot::new("waveforms")
                            .legend(Legend::default())
                            .x_axis_label("Time")
                            .y_axis_label("Amplitude, ADC")
                            .x_axis_formatter(|GridMark { value, .. }, _| {
                                format_time(nanoseconds(value))
                            })
                            .show(ui, |plot_ui| {
                                for (ch_num, offset, waveform) in chunks[self.current_chunk].clone()
                                {
                                    waveform.draw_egui(
                                        plot_ui,
                                        Some(&format!("ch #{}", ch_num + 1)),
                                        Some(color_for_index((ch_num) as usize)),
                                        None,
                                        Some(offset),
                                    );
                                }
                            });
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use numass_processing::types::second;

    /// Точка с одним каналом и кадрами с данными таймстампами (нс).
    fn point_with_times(times: &[u64]) -> rsb_event::Point {
        use rsb_event::point::{
            Channel,
            channel::{Block, block::Frame},
        };

        rsb_event::Point {
            channels: vec![Channel {
                id: 1,
                blocks: vec![Block {
                    time: 0,
                    frames: times
                        .iter()
                        .map(|&time| Frame {
                            time,
                            data: vec![0, 0],
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// Мусорный таймстамп, который correct_frame_time не исправляет:
    /// порог исправления — 0xf000_0000_0000_0000, эта константа ниже.
    const GARBAGE_TIME: u64 = 0x8000_0000_0000_0000;

    #[test]
    fn garbage_timestamps_are_repaired_not_dropped() {
        // Раньше кадр с таким таймстампом поднимал chunk_num до ~10^13
        // (capacity overflow на wasm32), а первый фикс отбрасывал его совсем.
        let point = point_with_times(&[
            0,
            1_500_000,
            // известная константа — correct_frame_time исправляет
            0xffff_fff9_03da_0000 + 2_500_000,
            // неизвестная — «ремонтируется», кадр рисуем
            GARBAGE_TIME,
        ]);
        let data = point_to_chunks(
            point,
            Time::new::<millisecond>(1.0),
            Some(Time::new::<second>(30.0)),
        );

        // все 4 кадра нарисованы; сломанный встал сразу за валидной шкалой
        assert_eq!(data.chunks.len(), 3);
        assert_eq!(data.chunks.iter().map(Vec::len).sum::<usize>(), 4);
        assert_eq!(data.chunks[2].len(), 2);
        assert_eq!(data.dropped, 0);
    }

    #[test]
    fn each_broken_segment_gets_its_own_anchor() {
        const GARBAGE_TIME_2: u64 = 0xc000_0000_0000_0000;
        let point = point_with_times(&[
            0,
            1_000_000,
            GARBAGE_TIME,
            GARBAGE_TIME + 500_000,
            GARBAGE_TIME_2,
            GARBAGE_TIME_2 + 500_000,
        ]);
        let data = point_to_chunks(
            point,
            Time::new::<millisecond>(1.0),
            Some(Time::new::<second>(30.0)),
        );

        // оба сегмента сбоя нарисованы, каждый — за концом предыдущей шкалы
        assert_eq!(data.chunks.len(), 3);
        assert_eq!(data.chunks.iter().map(Vec::len).sum::<usize>(), 6);
        assert_eq!(data.dropped, 0);
    }

    #[test]
    fn unrecoverable_frames_are_counted() {
        // сегмент сбоя «дрейфует» шагами меньше bound (31 с), но суммарный
        // сдвиг хвоста (40 с) его превышает: хвост — мусор внутри сегмента
        let point = point_with_times(&[
            0,
            1_000_000,
            GARBAGE_TIME,
            GARBAGE_TIME + 20_000_000_000,
            GARBAGE_TIME + 40_000_000_000,
        ]);
        let data = point_to_chunks(
            point,
            Time::new::<millisecond>(1.0),
            Some(Time::new::<second>(30.0)),
        );

        assert_eq!(data.dropped, 1);
        assert_eq!(data.chunks.iter().map(Vec::len).sum::<usize>(), 4);
    }

    #[test]
    fn shifted_timestamps_are_normalized_to_first_frame() {
        // ко всем временам примешана неизвестная константа:
        // чанки идут от начала точки, кадры не теряются
        let point = point_with_times(&[
            GARBAGE_TIME,
            GARBAGE_TIME + 500_000,
            GARBAGE_TIME + 1_500_000,
        ]);
        let data = point_to_chunks(
            point,
            Time::new::<millisecond>(1.0),
            Some(Time::new::<second>(30.0)),
        );

        assert_eq!(data.chunks.len(), 2);
        assert_eq!(data.chunks[0].len(), 2);
        assert_eq!(data.chunks[1].len(), 1);
        assert_eq!(data.dropped, 0);
    }

    #[test]
    fn empty_point_yields_single_empty_chunk() {
        let data = point_to_chunks(
            rsb_event::Point::default(),
            Time::new::<millisecond>(1.0),
            Some(Time::new::<second>(30.0)),
        );

        assert_eq!(data.chunks.len(), 1);
        assert!(data.chunks[0].is_empty());
        assert_eq!(data.dropped, 0);
    }

    /// Чанк с одним кадром, если `non_empty`, иначе пустой.
    fn make_chunks(mask: &[bool]) -> Vec<Chunk> {
        mask.iter()
            .map(|&non_empty| {
                if non_empty {
                    vec![(0, nanoseconds(0.0), ProcessedWaveform(vec![0.0]))]
                } else {
                    vec![]
                }
            })
            .collect()
    }

    #[test]
    fn next_skips_empty_chunks() {
        let chunks = make_chunks(&[true, false, false, true, false]);
        assert_eq!(next_non_empty_chunk(&chunks, 0), 3);
        assert_eq!(next_non_empty_chunk(&chunks, 1), 3);
        assert_eq!(next_non_empty_chunk(&chunks, 3), 3);
        assert_eq!(next_non_empty_chunk(&chunks, 4), 4);
    }

    #[test]
    fn prev_skips_empty_chunks() {
        let chunks = make_chunks(&[false, true, false, false, true]);
        assert_eq!(prev_non_empty_chunk(&chunks, 4), 1);
        assert_eq!(prev_non_empty_chunk(&chunks, 3), 1);
        assert_eq!(prev_non_empty_chunk(&chunks, 1), 1);
        assert_eq!(prev_non_empty_chunk(&chunks, 0), 0);
    }

    #[test]
    fn stays_when_no_non_empty_neighbor() {
        let chunks = make_chunks(&[false, false, false]);
        assert_eq!(next_non_empty_chunk(&chunks, 1), 1);
        assert_eq!(prev_non_empty_chunk(&chunks, 1), 1);

        let single = make_chunks(&[false]);
        assert_eq!(next_non_empty_chunk(&single, 0), 0);
        assert_eq!(prev_non_empty_chunk(&single, 0), 0);
    }
}

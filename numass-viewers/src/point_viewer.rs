use std::{path::PathBuf, sync::Arc};

use egui::{Visuals, mutex::Mutex};
use egui_plot::{GridMark, Legend};
use numass_processing::{
    numass::protos::rsb_event,
    storage::load_point,
    types::{ProcessedWaveform, RawWaveform, Time, millisecond, nanoseconds, time_ns},
    utils::{EguiLine, color_for_index, correct_frame_time, format_time},
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
    chunks: Arc<Mutex<Option<Vec<Chunk>>>>,
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
fn point_to_chunks(point: rsb_event::Point, limit: Time) -> Vec<Chunk> {
    let limit_ns = time_ns(limit);
    let frames = frames_with_times(&point);

    let mut chunks = vec![vec![]];
    for (time, channel, frame) in frames {
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

    chunks
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
            chunks: Arc::new(Mutex::new(None)),
            current_chunk: 0,
            state: Arc::new(Mutex::new(AppState::Initializing)),
        };

        let chunks = Arc::clone(&viewer.chunks);
        let state = Arc::clone(&viewer.state);

        spawn(async move {
            let point = load_point(&filepath).await;
            *chunks.lock() = Some(point_to_chunks(point, Time::new::<millisecond>(1.0)));
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
                if let Some(chunks) = self.chunks.lock().as_ref() {
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

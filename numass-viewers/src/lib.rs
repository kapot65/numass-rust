#![warn(clippy::all, rust_2018_idioms)]
use std::{path::PathBuf, sync::Arc};

use app::ProcessingStatus;
use egui::mutex::Mutex;
use numass_processing::utils::format_time;
use numass_processing::{utils::events_to_histogram, viewer::EMPTY_POINT};

use numass_processing::{
    histogram::HistogramParams, postprocess::PostProcessParams, preprocess::PreprocessParams,
    process::ProcessParams, types::nanoseconds, viewer::PointState,
};

pub mod app;
pub mod bundle_viewer;
pub mod filtered_viewer;
pub mod point_viewer;
pub mod trigger_viewer;

/// Статистика сломанных таймстампов точки: сколько поправлено и сколько
/// отброшено (ремонт — `numass_processing::utils::repair_frame_times`).
#[derive(Debug, Clone, Default)]
pub(crate) struct TimestampStats {
    /// Сколько таймстампов поправлено ремонтом.
    pub(crate) repaired: usize,
    /// Постоянный шаг в наносекундах, вычисаемый из всех таймстампов
    /// сегмента сбоя; по одному на сегмент.
    pub(crate) shifts_ns: Vec<u64>,
    /// Сколько кадров/событий не отрисовано из-за невосстановимого времени.
    pub(crate) dropped: usize,
}

impl TimestampStats {
    /// Короткая сводка по таймстампам для подписи у графика; `None`, если
    /// ничего не ломалось.
    ///
    /// Формат: «3 timestamps repaired (shift 2.5 s), 1 frame dropped»;
    /// при нескольких сегментах сбоя у каждого своя поправка:
    /// «… repaired (shifts 1 ms, 8 ms)». `noun` — отбрасываемое в
    /// единственном числе (frame/event), зависит от вьювера.
    pub(crate) fn label(&self, noun: &str) -> Option<String> {
        let mut parts = Vec::new();

        if self.repaired > 0 {
            let shifts = self
                .shifts_ns
                .iter()
                .map(|&shift| format_time(nanoseconds(shift as f64)))
                .collect::<Vec<_>>();
            let repaired = match shifts.as_slice() {
                // построением у каждого поправленного сегмента есть свой шаг
                [] => format!("{} repaired", noun_phrase(self.repaired, "timestamp")),
                [shift] => format!(
                    "{} repaired (shift {})",
                    noun_phrase(self.repaired, "timestamp"),
                    shift
                ),
                shifts => format!(
                    "{} repaired (shifts {})",
                    noun_phrase(self.repaired, "timestamp"),
                    shifts.join(", ")
                ),
            };
            parts.push(repaired);
        }

        if self.dropped > 0 {
            parts.push(format!("{} dropped", noun_phrase(self.dropped, noun)));
        }

        (!parts.is_empty()).then(|| parts.join(", "))
    }
}

/// Существительное, согласованное с числом: «1 frame», «2 events».
fn noun_phrase(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// Данные, разложенные по чанкам, плюс статистика таймстампов.
pub(crate) struct ChunkedData<T> {
    pub(crate) chunks: Vec<T>,
    pub(crate) stats: TimestampStats,
}

/// Program version, taken from the crate metadata at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `true` when the program was built from a working tree that had uncommitted
/// changes to tracked files.
pub fn is_dirty() -> bool {
    matches!(option_env!("GIT_DIRTY"), Some("1"))
}

/// Draws a small, left-aligned label with the program version under a plot.
/// When the build came from a dirty working tree, an additional marker is
/// shown; hovering the version reveals the exact commit (`git describe`).
pub fn version_label(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("v{VERSION}")).small().weak())
            .on_hover_text(format!(
                "git: {}",
                option_env!("GIT_DESCRIBE").unwrap_or("unknown")
            ));

        if is_dirty() {
            ui.label(
                egui::RichText::new("uncommitted changes")
                    .small()
                    .color(egui::Color32::from_rgb(220, 160, 0)),
            )
            .on_hover_text("the working tree had uncommitted changes when this version was built");
        }
    });
}

/// Increment processed files counter and reset it if it is finished.
pub fn inc_status(status: Arc<Mutex<ProcessingStatus>>) {
    let mut status = status.lock();
    status.processed += 1;
    if status.processed == status.total {
        *status = ProcessingStatus {
            running: false,
            total: 0,
            processed: 0,
        }
    }
}

#[cfg(target_arch = "wasm32")]
use gloo::worker::oneshot::oneshot;

#[cfg(target_arch = "wasm32")]
#[oneshot]
pub async fn PointProcessor(
    args: (
        PathBuf,
        PreprocessParams,
        ProcessParams,
        PostProcessParams,
        HistogramParams,
    ),
) -> Option<PointState> {
    let (filepath, preprocess, process, post_process, histogram) = args;
    process_point(filepath, preprocess, process, post_process, histogram).await
}

pub async fn process_point(
    filepath: PathBuf,
    preprocess: PreprocessParams,
    process: ProcessParams,
    post_process: PostProcessParams,
    histogram: HistogramParams,
) -> Option<PointState> {
    let modified = numass_processing::storage::load_modified_time(filepath.clone()).await; // TODO: remove clone

    let events = numass_processing::storage::process_point(
        &filepath,
        &preprocess,
        &process,
        Some(&post_process),
    )
    .await
    // TODO(К3): ошибку скрипта/данных показать пользователю через PointState.error
    // вместо паники.
    .unwrap_or_else(|err| panic!("{err}"));

    events.map(|(_, events)| {
        if let Some((events, preprocess)) = events {
            let histogram = events_to_histogram(events, histogram);

            let counts = Some(histogram.events_all(None));

            PointState {
                opened: true,
                histogram: Some(histogram),
                preprocess: Some(preprocess),
                modified,
                counts,
            }
        } else {
            EMPTY_POINT
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_hidden_when_nothing_broken() {
        assert_eq!(TimestampStats::default().label("frame"), None);
    }

    #[test]
    fn label_reports_repaired_and_dropped() {
        let stats = TimestampStats {
            repaired: 3,
            shifts_ns: vec![2_500_000_000],
            dropped: 1,
        };

        assert_eq!(
            stats.label("frame").as_deref(),
            Some("3 timestamps repaired (shift 2.5 s), 1 frame dropped")
        );
    }

    #[test]
    fn label_uses_singular_forms() {
        let stats = TimestampStats {
            repaired: 1,
            shifts_ns: vec![1_000],
            dropped: 0,
        };

        assert_eq!(
            stats.label("event").as_deref(),
            Some("1 timestamp repaired (shift 1 µs)")
        );
    }

    #[test]
    fn label_lists_every_shift() {
        let stats = TimestampStats {
            repaired: 6,
            shifts_ns: vec![1_000_000, 8_000_000],
            dropped: 0,
        };

        assert_eq!(
            stats.label("frame").as_deref(),
            Some("6 timestamps repaired (shifts 1 ms, 8 ms)")
        );
    }

    #[test]
    fn label_counts_dropped_only() {
        let stats = TimestampStats {
            dropped: 2,
            ..TimestampStats::default()
        };

        assert_eq!(stats.label("event").as_deref(), Some("2 events dropped"));
    }
}

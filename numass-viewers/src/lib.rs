#![warn(clippy::all, rust_2018_idioms)]
use std::{path::PathBuf, sync::Arc};

use app::ProcessingStatus;
use egui::mutex::Mutex;
use numass_processing::{utils::events_to_histogram, viewer::EMPTY_POINT};

use numass_processing::{
    histogram::HistogramParams, postprocess::PostProcessParams, preprocess::PreprocessParams,
    process::ProcessParams, viewer::PointState,
};

pub mod app;
pub mod bundle_viewer;
pub mod filtered_viewer;
pub mod point_viewer;
pub mod trigger_viewer;

/// Данные, разложенные по чанкам, плюс статистика отброшенного.
///
/// `dropped` — сколько кадров/событий не отрисовано из-за невосстановимого
/// времени: мусорного таймстампа, который не «ремонтирует"
/// `numass_processing::utils::repair_frame_times`.
pub(crate) struct ChunkedData<T> {
    pub(crate) chunks: Vec<T>,
    pub(crate) dropped: usize,
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
    .await;

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

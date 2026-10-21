use std::{path::PathBuf, sync::Arc};

use crate::TimestampStats;
use egui::{Color32, Visuals, mutex::Mutex};
use egui_plot::{GridMark, Legend, VLine};
use numass_processing::{
    histogram::PointHistogram,
    numass::{NumassMeta, Reply, protos::rsb_event},
    preprocess::{CUTOFF_BIN_SIZE, Preprocess},
    storage::{load_meta, load_point},
    types::{Time, millisecond, nanosecond, nanoseconds, second, time_ns},
    utils::{correct_frame_time, format_time, repair_frame_times, timestamps_bound},
};

#[cfg(not(target_arch = "wasm32"))]
use tokio::spawn;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local as spawn;

pub struct TriggerViewer {
    meta: Arc<Mutex<Option<NumassMeta>>>,
    point: Arc<Mutex<Option<rsb_event::Point>>>,
    trigger_density: Arc<Mutex<Option<PointHistogram>>>,
    stats: Arc<Mutex<TimestampStats>>,
    preprocess: Arc<Mutex<Option<Preprocess>>>,

    /// bin size in ms
    bin_size: u64,
    per_channel: bool,
}

impl TriggerViewer {
    pub fn init_with_point(filepath: PathBuf) -> Self {
        let viewer = TriggerViewer {
            meta: Arc::new(Mutex::new(None)),
            point: Arc::new(Mutex::new(None)),
            stats: Arc::new(Mutex::new(TimestampStats::default())),
            preprocess: Arc::new(Mutex::new(None)),

            trigger_density: Arc::new(Mutex::new(None)),
            bin_size: 10,
            per_channel: false,
        };

        let meta = Arc::clone(&viewer.meta);
        let point = Arc::clone(&viewer.point);
        let trigger_density = Arc::clone(&viewer.trigger_density);
        let stats = Arc::clone(&viewer.stats);
        let static_params = Arc::clone(&viewer.preprocess);
        let limit_ms = viewer.bin_size;

        spawn(async move {
            let meta_local = load_meta(&filepath).await;
            meta.lock().clone_from(&meta_local);

            let point_local = load_point(&filepath).await;

            // TODO: optimize to prevent double processing of point data
            let static_params_local = Preprocess::from_point(meta_local.clone(), &point_local);
            *static_params.lock() = Some(static_params_local);

            *point.lock() = Some(point_local);

            if let Some(NumassMeta::Reply(Reply::AcquirePoint {
                acquisition_time, ..
            })) = meta_local
            {
                TriggerViewer::calc_density(
                    point,
                    trigger_density,
                    stats,
                    Time::new::<millisecond>(limit_ms as f64),
                    Time::new::<second>(f64::from(acquisition_time)),
                )
                .await;
            } else {
                panic!("Unexpected meta data type")
            }
        });

        viewer
    }

    /// Calculates the density of triggers over time.
    ///
    /// Времена кадров проходят тот же ремонт, что и в остальных вьюверах
    /// ([`repair_frame_times`]): сегменты сбоя ставятся следом за валидной
    /// шкалой, мусор отбрасывается. Ось растягивается до конца
    /// отремонтированной шкалы, чтобы поправленные триггеры были видны.
    ///
    /// # Arguments
    /// * `bin_size` - A size of each time bin.
    /// * `acquisition_time` - Total acquisition time.
    ///
    async fn calc_density(
        point: Arc<Mutex<Option<rsb_event::Point>>>,
        trigger_density: Arc<Mutex<Option<PointHistogram>>>,
        stats: Arc<Mutex<TimestampStats>>,
        bin_size: Time,
        acquisition_time: Time,
    ) {
        if let Some(point) = point.lock().as_ref() {
            let bound_ns = timestamps_bound(acquisition_time);

            // (канал, время кадра) всех кадров; ось плотности — наносекунды
            let frames: Vec<(u8, u64)> = point
                .channels
                .iter()
                .flat_map(|channel| {
                    channel
                        .blocks
                        .iter()
                        .flat_map(|block| block.frames.iter())
                        .map(move |frame| (channel.id as u8, correct_frame_time(frame.time)))
                })
                .collect();
            let times: Vec<u64> = frames.iter().map(|&(_, time)| time).collect();
            let repair = repair_frame_times(&times, bound_ns);

            let range_end = repair
                .times
                .iter()
                .flatten()
                .copied()
                .max()
                .unwrap_or(0)
                .max(time_ns(acquisition_time))
                + 1; // add() не считает значение точно на границе
            let mut trigger_density_local = PointHistogram::new_step(
                0.0..range_end as f32,
                bin_size.get::<nanosecond>() as f32,
            );

            let mut dropped = 0;
            for ((channel, _), time) in frames.iter().zip(&repair.times) {
                match time {
                    Some(time) => trigger_density_local.add(*channel, *time as f32),
                    None => dropped += 1,
                }
            }

            *stats.lock() = TimestampStats {
                repaired: repair.repaired,
                shifts_ns: repair.shifts_ns,
                dropped,
            };
            *trigger_density.lock() = Some(trigger_density_local);
        }
    }
}

impl eframe::App for TriggerViewer {
    #[allow(unused_variables)]
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.set_visuals(Visuals::dark());

        egui::SidePanel::left("left").show(ctx, |ui| {
            egui::TopBottomPanel::bottom("version").show_inside(ui, |ui| {
                crate::version_label(ui);
            });

            ui.add(egui::Slider::new(&mut self.bin_size, 1..=1_000).text("bin size (ms)"));
            ui.checkbox(&mut self.per_channel, "show each channel");
            if ui.button("apply").clicked() {
                *self.trigger_density.lock() = None;
                *self.stats.lock() = TimestampStats::default();

                let point = Arc::clone(&self.point);
                let trigger_density = Arc::clone(&self.trigger_density);
                let stats = Arc::clone(&self.stats);
                let limit_ms = self.bin_size;

                let meta = self.meta.lock().clone();
                spawn(async move {
                    if let Some(NumassMeta::Reply(Reply::AcquirePoint {
                        acquisition_time, ..
                    })) = meta
                    {
                        TriggerViewer::calc_density(
                            point,
                            trigger_density,
                            stats,
                            Time::new::<millisecond>(limit_ms as f64),
                            Time::new::<second>(f64::from(acquisition_time)),
                        )
                        .await;
                    } else {
                        panic!("Unexpected meta data type")
                    }
                });
            }
            ui.separator();

            if let Some(NumassMeta::Reply(Reply::AcquirePoint {
                acquisition_time, ..
            })) = self.meta.lock().as_ref()
            {
                ui.label(format!(
                    "acquisition_time: {}",
                    format_time(Time::new::<second>(f64::from(*acquisition_time)))
                ));
            }
        });

        if let Some(trigger_density) = self.trigger_density.lock().as_ref() {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(label) = self.stats.lock().label("frame") {
                    ui.weak(label);
                }

                egui_plot::Plot::new("triggers")
                    .legend(Legend::default())
                    .x_axis_label("Time")
                    .x_axis_formatter(|GridMark { value, .. }, _| format_time(nanoseconds(value)))
                    .show(ui, |plot_ui| {
                        if self.per_channel {
                            trigger_density.draw_egui_each_channel(plot_ui, None);
                        } else {
                            trigger_density.draw_egui(plot_ui, None, None, None)
                        }

                        if let Some(Preprocess { bad_blocks, .. }) =
                            &self.preprocess.lock().as_ref()
                        {
                            bad_blocks.iter().for_each(|idx| {
                                plot_ui.vline(
                                    VLine::new("BAD", CUTOFF_BIN_SIZE as f64 * (*idx as f64))
                                        .color(Color32::WHITE),
                                );
                                plot_ui.vline(
                                    VLine::new("BAD", CUTOFF_BIN_SIZE as f64 * ((*idx + 1) as f64))
                                        .color(Color32::WHITE),
                                );
                            });
                        }
                    });
            });
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.spinner();
            });
        }
    }
}

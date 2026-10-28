use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use eframe::egui;
use egui::mutex::Mutex;
use egui_plot::{Legend, Plot};

use numass_processing::histogram::{HistogramParams, PointHistogram};
use numass_processing::preprocess::{PreprocessParams, apply_frame, draw_processed_frame};
use numass_processing::process::{Algorithm, ProcessParams, extract_frame};
use numass_processing::types::{
    FrameEvent, NumassFrame, NumassFrameFast, ProcessedWaveform, nanoseconds,
};
use numass_processing::utils::{EguiLine, color_for_index, format_time};
use numass_processing::widgets::{HISTOGRAM_MAX, HISTOGRAM_MIN, UserInput};
use numass_tqdc::mlink::MlinkMessage;
use numass_tqdc::mstream::{ADCDataBlock, MStreamTriggerAndUserData};
use numass_tqdc::{MTU_SIZE, TQDC_STREAM_PORT};
use numass_tqdc_apps::defaults::BOARD_IP;

#[cfg(target_family = "unix")]
use tikv_jemallocator::Jemalloc;
#[cfg(target_family = "unix")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = BOARD_IP)]
    tqdc_ip: std::net::IpAddr,

    #[arg(long, default_value_t = 25.0)]
    hist_min: f32,

    #[arg(long, default_value_t = 400.0)]
    hist_max: f32,

    /// Lowest value the histogram left/right sliders can take.
    #[arg(long, default_value_t = HISTOGRAM_MIN)]
    hist_bound_min: f32,

    /// Highest value the histogram left/right sliders can take.
    #[arg(long, default_value_t = HISTOGRAM_MAX)]
    hist_bound_max: f32,

    #[arg(long, default_value_t = 400)]
    hist_bins: usize,

    /// graphics refresh rate in Hz
    #[arg(long, default_value_t = 1.0)]
    refresh_rate: f32,

    #[arg(long, default_value_t = 25.0)]
    count_rate_threshold: f32,

    #[command(subcommand)]
    algorithm: Option<Algorithm>,
}

fn main() {
    let Args {
        tqdc_ip,
        hist_min,
        hist_max,
        hist_bound_min,
        hist_bound_max,
        hist_bins,
        refresh_rate,
        count_rate_threshold,
        algorithm,
    } = Args::parse();

    let (bound_min, bound_max) = if hist_bound_min <= hist_bound_max {
        (hist_bound_min, hist_bound_max)
    } else {
        (hist_bound_max, hist_bound_min)
    };
    let hist_min = hist_min.clamp(bound_min, bound_max);
    let hist_max = hist_max.clamp(bound_min, bound_max);
    let (hist_min, hist_max) = if hist_min < hist_max {
        (hist_min, hist_max)
    } else {
        (bound_min, bound_max)
    };
    let histogram_initial = HistogramParams {
        range: hist_min..hist_max,
        bins: hist_bins.clamp(10, 2000),
    };

    let algorithm = algorithm.unwrap_or_default();
    let process_params = Arc::new(Mutex::new(ProcessParams {
        algorithm: algorithm.clone(),
        convert_to_kev: false,
    }));
    let process_params_ui = Arc::clone(&process_params);
    let preprocess_initial = PreprocessParams::default();
    let preprocess_params = Arc::new(Mutex::new(preprocess_initial.clone()));
    let preprocess_params_ui = Arc::clone(&preprocess_params);

    let histogram_bg = Arc::new(Mutex::new(PointHistogram::from(histogram_initial.clone())));

    let histogram = Arc::clone(&histogram_bg);

    let channels_bg = Arc::new(Mutex::new(BTreeMap::new()));
    let channels = Arc::clone(&channels_bg);

    let raw_frame_bg = Arc::new(Mutex::new(None::<NumassFrame>));
    let raw_frame = Arc::clone(&raw_frame_bg);

    let count_rate_bg = Arc::new(Mutex::new(BTreeMap::new()));
    let count_rate = Arc::clone(&count_rate_bg);

    let packet_rate_bg = Arc::new(Mutex::new(0.0f64));
    let packet_rate = Arc::clone(&packet_rate_bg);

    std::thread::spawn(move || {
        let tqdc_address = SocketAddr::new(tqdc_ip, TQDC_STREAM_PORT);

        let sock = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();

        sock.send_to(
            &MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
                0x0000, 0x0001, 0xfefe, 0xFFFF, 0xFFFF,
            )),
            tqdc_address,
        )
        .unwrap();

        let mut refresh_timer = Instant::now();

        let counts = Arc::new(Mutex::new(BTreeMap::new()));

        let mut fragments_buf: BTreeMap<u16, BTreeMap<u16, numass_tqdc::mstream::MStreamFragment>> =
            BTreeMap::new();

        let mut seq = 0;
        let mut fragment_count = 0u64;

        loop {
            let mut buf = [0; MTU_SIZE];
            let (len, to_addr) = sock.recv_from(&mut buf).unwrap();
            let message = MlinkMessage::from_datagram(&buf[..len]);

            let plots_refresh_interval_ms = (1.0 / refresh_rate * 1000.0) as u128;

            match message {
                MlinkMessage::StreamReq { fragment, .. } => {
                    sock.send_to(
                        &MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
                            seq,
                            0x0001,
                            0xfefe,
                            fragment.header.fragment_offset,
                            fragment.header.fragment_id,
                        )),
                        to_addr,
                    )
                    .unwrap();
                    seq += 1;
                    fragment_count += 1;

                    fragments_buf
                        .entry(fragment.header.fragment_id)
                        .or_default()
                        .insert(fragment.header.fragment_offset, fragment);

                    let elapsed_ms = refresh_timer.elapsed().as_millis();
                    if elapsed_ms > plots_refresh_interval_ms {
                        let packet_rate_val = fragment_count as f64 / (elapsed_ms as f64 / 1000.0);
                        *packet_rate_bg.lock() = packet_rate_val;
                        fragment_count = 0;

                        let channels_bg = Arc::clone(&channels_bg);
                        let histogram_bg = Arc::clone(&histogram_bg);
                        let counts = Arc::clone(&counts);

                        let count_rate_bg = Arc::clone(&count_rate_bg);
                        let process_params = Arc::clone(&process_params);
                        let preprocess_params = Arc::clone(&preprocess_params);
                        let raw_frame_bg = Arc::clone(&raw_frame_bg);

                        fragments_buf = {
                            std::thread::spawn(move || {
                                let mut counts = counts.lock();
                                let mut histogram = histogram_bg.lock();

                                let algorithm = process_params.lock().algorithm.clone();
                                let steps = preprocess_params.lock().steps.clone();

                                let mut channels = None;
                                let mut raw_frame = None;

                                fragments_buf.iter().for_each(|(_, fragments)| {
                                    let fragments = fragments.values().collect::<Vec<_>>();
                                    if let Ok(combined) =
                                        MStreamTriggerAndUserData::try_from(&fragments[..])
                                    {
                                        let data_blocks = combined.extract_data_blocks();
                                        let frame: NumassFrameFast = data_blocks
                                            .iter()
                                            .map(|ADCDataBlock { ch_num, waveform }| {
                                                (*ch_num, waveform.as_slice())
                                            })
                                            .collect::<BTreeMap<_, _>>();

                                        // no-GUI обработка: ошибка скрипта — паника, как раньше.
                                        let processed =
                                            apply_frame(&frame, &steps).expect("preprocess");
                                        let events = extract_frame(
                                            &processed, &frame, &algorithm, &mut None,
                                        )
                                        .expect("process");

                                        events.into_iter().for_each(|(_, event)| {
                                            if let FrameEvent::Event {
                                                channel, amplitude, ..
                                            } = event
                                            {
                                                if amplitude > count_rate_threshold {
                                                    *counts.entry(channel).or_insert(0.0f64) += 1.0;
                                                }
                                                histogram.add(channel, amplitude);
                                            }
                                        });

                                        if channels.is_none() {
                                            channels = Some(
                                                frame
                                                    .iter()
                                                    .map(|(ch_num, waveform)| {
                                                        (
                                                            *ch_num,
                                                            ProcessedWaveform::from(*waveform),
                                                        )
                                                    })
                                                    .collect::<BTreeMap<_, _>>(),
                                            );
                                            raw_frame = Some(
                                                frame
                                                    .iter()
                                                    .map(|(ch_num, waveform)| {
                                                        (*ch_num, waveform.to_vec())
                                                    })
                                                    .collect::<BTreeMap<_, _>>(),
                                            );
                                        }
                                    } else {
                                        println!("failed to combine fragments");
                                        fragments.iter().for_each(|fr| println!("{:?}", fr.header));
                                        println!("=====");
                                    }
                                });

                                if let Some(channels) = channels {
                                    *channels_bg.lock() = channels;
                                }

                                if let Some(raw_frame) = raw_frame {
                                    *raw_frame_bg.lock() = Some(raw_frame);
                                }

                                counts.iter_mut().for_each(|(_, count)| {
                                    *count /= (elapsed_ms) as f64 / 1000.0;
                                });

                                *count_rate_bg.lock() = counts.clone();
                                counts.clear();

                                fragments_buf.clear();
                            });

                            refresh_timer = Instant::now();
                            BTreeMap::new()
                        };
                    }
                }
                _ => panic!("unimplemented!!"),
            }
        }
    });

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "frame-viewer",
        native_options,
        Box::new(|_| {
            Ok(Box::new(MyEguiApp {
                channels,
                raw_frame,
                histogram,
                count_rate,
                packet_rate,
                process_params: process_params_ui,
                process_params_edit: ProcessParams {
                    algorithm: algorithm.clone(),
                    convert_to_kev: false,
                },
                preprocess_params: preprocess_params_ui,
                preprocess_edit: preprocess_initial,
                histogram_edit: histogram_initial,
                hist_bound: bound_min..=bound_max,
                draw_algorithm: false,
                params_open: true,
                params_width: PARAMS_MIN_WIDTH,
                signals_frac: 0.5,
            }))
        }),
    )
    .unwrap();
}

/// Starting params column. Sliders stay readable, the rest wraps.
const PARAMS_MIN_WIDTH: f32 = 220.0;
/// Collapsed params column: just the expand button.
const PARAMS_COLLAPSED: f32 = 28.0;
/// Gap that holds a drag handle between columns.
const SPLIT_GAP: f32 = 6.0;
/// Do not let a drag squash a plot below this.
const MIN_PLOT: f32 = 80.0;

/// Program version from the crate manifest. Hover shows `git describe`.
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn is_dirty() -> bool {
    matches!(option_env!("GIT_DIRTY"), Some("1"))
}

/// Same label as the other viewers: small version, plus a marker when the
/// build came from a dirty working tree. Sits at the bottom of the params column.
fn version_label(ui: &mut egui::Ui) {
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

struct MyEguiApp {
    histogram: Arc<Mutex<PointHistogram>>,
    channels: Arc<Mutex<BTreeMap<u8, ProcessedWaveform>>>,
    raw_frame: Arc<Mutex<Option<NumassFrame>>>,
    count_rate: Arc<Mutex<BTreeMap<u8, f64>>>,
    packet_rate: Arc<Mutex<f64>>,
    process_params: Arc<Mutex<ProcessParams>>,
    process_params_edit: ProcessParams,
    preprocess_params: Arc<Mutex<PreprocessParams>>,
    preprocess_edit: PreprocessParams,
    histogram_edit: HistogramParams,
    /// Slider limits for the histogram edges, from `--hist-bound-min/max`.
    hist_bound: std::ops::RangeInclusive<f32>,
    draw_algorithm: bool,
    params_open: bool,
    /// Params column width. Starts at [`PARAMS_MIN_WIDTH`], then follows the drag.
    params_width: f32,
    /// Share of the leftover width taken by the waveform plot. Starts at half.
    signals_frac: f32,
}

/// Child that fills `rect` and does not advance the parent cursor.
fn show_column(
    ui: &mut egui::Ui,
    id: impl egui::AsIdSalt,
    rect: egui::Rect,
    add: impl FnOnce(&mut egui::Ui),
) {
    let mut column = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    column.shrink_clip_rect(rect);
    add(&mut column);
}

/// `egui_plot` paints the Y title left of the plot frame. Shift the frame in by one
/// line so the title stays inside the column instead of covering the column to its left.
fn show_plot(ui: &mut egui::Ui, id: impl egui::AsIdSalt, plot: impl FnOnce(&mut egui::Ui)) {
    let gutter = ui.text_style_height(&egui::TextStyle::Body) * 1.5;
    let rect = egui::Rect::from_min_max(
        egui::pos2(ui.max_rect().left() + gutter, ui.cursor().min.y),
        ui.max_rect().max,
    );
    let mut plot_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    plot(&mut plot_ui);
}

/// Vertical drag handle centered on `x`. Returns how far it moved this frame.
fn splitter(ui: &mut egui::Ui, id: impl egui::AsIdSalt, x: f32, y0: f32, y1: f32) -> f32 {
    let grab = ui.style().interaction.resize_grab_radius_side;
    let rect = egui::Rect::from_min_max(egui::pos2(x - grab, y0), egui::pos2(x + grab, y1));
    let response = ui.interact(rect, ui.id().with(id), egui::Sense::drag());
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    let stroke = if response.dragged() {
        ui.style().visuals.widgets.active.fg_stroke
    } else if response.hovered() {
        ui.style().visuals.widgets.hovered.fg_stroke
    } else {
        ui.style().visuals.widgets.noninteractive.bg_stroke
    };
    ui.painter().vline(x, y0..=y1, stroke);
    response.drag_delta().x
}

impl eframe::App for MyEguiApp {
    /// Widths live on the app, not in egui panel state, so a previous run cannot bring them back.
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(std::time::Duration::from_millis(1000 / 60));

        let channels = self.channels.lock().clone();

        let area = ui.available_rect_before_wrap();
        let params_width = if self.params_open {
            let room = (area.width() - 2.0 * SPLIT_GAP - 2.0 * MIN_PLOT).max(PARAMS_MIN_WIDTH);
            self.params_width.min(room)
        } else {
            PARAMS_COLLAPSED
        };
        let plots_width = (area.width() - params_width - 2.0 * SPLIT_GAP).max(1.0);
        let signals_width = if plots_width <= 2.0 * MIN_PLOT {
            plots_width * self.signals_frac
        } else {
            (plots_width * self.signals_frac).clamp(MIN_PLOT, plots_width - MIN_PLOT)
        };

        let params_rect =
            egui::Rect::from_min_size(area.min, egui::vec2(params_width, area.height()));
        let signals_x = params_rect.right() + SPLIT_GAP;
        let signals_rect = egui::Rect::from_min_size(
            egui::pos2(signals_x, area.top()),
            egui::vec2(signals_width, area.height()),
        );
        let hist_x = (signals_rect.right() + SPLIT_GAP).min(area.right());
        let hist_rect = egui::Rect::from_min_max(egui::pos2(hist_x, area.top()), area.max);

        if self.params_open {
            show_column(ui, "params", params_rect, |ui| self.params_body(ui));
        } else {
            show_column(ui, "params-collapsed", params_rect, |ui| {
                if ui
                    .add(egui::Button::new(">").frame(false))
                    .on_hover_text("Show processing params")
                    .clicked()
                {
                    self.params_open = true;
                }
            });
        }

        show_column(ui, "signals", signals_rect, |ui| {
            show_plot(ui, "waveforms-gutter", |ui| {
                Plot::new("waveforms")
                    .legend(Legend::default())
                    .x_axis_label("Time")
                    .y_axis_label("Amplitude, ADC")
                    .x_axis_formatter(|mark, _| format_time(nanoseconds(mark.value)))
                    .show(ui, |plot_ui| {
                        channels.into_iter().for_each(|(ch_num, waveform)| {
                            waveform.draw_egui(
                                plot_ui,
                                Some(&format!("ch #{}", ch_num + 1)),
                                Some(color_for_index(ch_num as usize)),
                                None,
                                None,
                            );
                        });

                        if self.draw_algorithm
                            && let Some(frame) = self.raw_frame.lock().clone()
                        {
                            let frame_fast: NumassFrameFast = frame
                                .iter()
                                .map(|(ch_num, waveform)| (*ch_num, waveform.as_slice()))
                                .collect();
                            let algorithm = self.process_params.lock().algorithm.clone();
                            let steps = self.preprocess_params.lock().steps.clone();
                            // GUI, но скрипт-шаг здесь не редактируется (tqdc-вьювер);
                            // ошибка скрипта/данных — паника, как раньше.
                            // TODO: показать в UI, когда скрипт-шаг появится здесь.
                            let processed = apply_frame(&frame_fast, &steps).expect("preprocess");
                            draw_processed_frame(&processed, plot_ui);
                            extract_frame(&processed, &frame_fast, &algorithm, &mut Some(plot_ui))
                                .expect("process");
                        }
                    });
            });
        });

        show_column(ui, "histogram", hist_rect, |ui| {
            ui.horizontal(|ui| {
                if ui.button("clear").clicked() {
                    self.histogram.lock().clear();
                }
            });

            show_plot(ui, "hists-gutter", |ui| {
                Plot::new("hists")
                    .legend(Legend::default())
                    .x_axis_label("amplitude")
                    .y_axis_label("counts")
                    .show(ui, |plot_ui| {
                        let hist = self.histogram.lock().clone();
                        hist.draw_egui_each_channel(plot_ui, None);
                    });
            });
        });

        // After the columns, so a plot cannot eat the drag.
        if self.params_open {
            let params_dx = splitter(
                ui,
                "split-params",
                params_rect.right() + SPLIT_GAP / 2.0,
                area.top(),
                area.bottom(),
            );
            if params_dx != 0.0 {
                let room = area.width() - 2.0 * SPLIT_GAP - 2.0 * MIN_PLOT;
                self.params_width = (self.params_width + params_dx)
                    .clamp(PARAMS_MIN_WIDTH, room.max(PARAMS_MIN_WIDTH));
            }
        }

        let signals_dx = splitter(
            ui,
            "split-signals",
            signals_rect.right() + SPLIT_GAP / 2.0,
            area.top(),
            area.bottom(),
        );
        if signals_dx != 0.0 && plots_width > 0.0 {
            let next = (signals_width + signals_dx) / plots_width;
            let lo = (MIN_PLOT / plots_width).min(0.5);
            self.signals_frac = next.clamp(lo, 1.0 - lo);
        }
    }
}

impl MyEguiApp {
    /// Params column: collapse button, forms, then the version pinned to the bottom.
    fn params_body(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let full = ui.max_rect();

        let version_h = ui.spacing().interact_size.y;
        let version_rect =
            egui::Rect::from_min_max(egui::pos2(full.left(), full.bottom() - version_h), full.max);
        let body_rect = egui::Rect::from_min_max(full.min, version_rect.right_top());

        let mut body = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("params-scroll")
                .max_rect(body_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        egui::ScrollArea::vertical()
            .id_salt("params-scroll-area")
            .auto_shrink([false, false])
            .show(&mut body, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    if ui
                        .add(egui::Button::new("<").frame(false))
                        .on_hover_text("Collapse")
                        .clicked()
                    {
                        self.params_open = false;
                    }
                });
                ui.separator();

                self.preprocess_edit = self.preprocess_edit.input(ui, &ctx);
                ui.separator();
                self.process_params_edit = self.process_params_edit.input(ui, &ctx);
                ui.separator();
                self.histogram_edit = self
                    .histogram_edit
                    .input_bounded(ui, self.hist_bound.clone());
                ui.separator();

                if ui.button("apply").clicked() {
                    *self.process_params.lock() = self.process_params_edit.clone();
                    *self.preprocess_params.lock() = self.preprocess_edit.clone();
                    // New binning starts empty. Old frames are not replayed.
                    *self.histogram.lock() = PointHistogram::from(self.histogram_edit.clone());
                }

                ui.separator();
                ui.checkbox(&mut self.draw_algorithm, "draw algorithm work");
                ui.separator();

                {
                    let count_rate_lock = self.count_rate.lock();
                    ui.label("Count rates");
                    for (ch_num, count_rate) in count_rate_lock.iter() {
                        ui.label(format!("ch {}: {count_rate:>8.2} Hz", ch_num + 1));
                    }
                }
                let packet_rate = *self.packet_rate.lock();
                ui.label(format!("packets: {packet_rate:>8.2} Hz"));
            });

        let mut version = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("version")
                .max_rect(version_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        version_label(&mut version);
    }
}

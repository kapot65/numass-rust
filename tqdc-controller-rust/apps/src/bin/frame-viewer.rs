use std::collections::BTreeMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use clap::Parser;
use eframe::egui;
use eframe::egui::plot::{Legend, Plot};

use apps::defaults::BOARD_IP;
use processing::{process_waveform, waveform_to_events, Algorithm, ProcessedWaveform, EguiLine, color_for_index};
use processing::histogram::PointHistogram;
use tqdc::{MTU_SIZE, TQDC_STREAM_PORT};
use tqdc::mlink::MlinkMessage;
use tqdc::mstream::{MStreamTriggerAndUserData, ADCDataBlock};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = BOARD_IP)]
    tqdc_ip: std::net::IpAddr,

    #[arg(long, default_value_t = 0.0)]
    hist_min: f32,

    #[arg(long, default_value_t = 400.0)]
    hist_max: f32,

    #[arg(long, default_value_t = 400)]
    hist_bins: usize,

    /// count rate accumulating interval in seconds
    #[arg(long, default_value_t = 5.0)]
    count_rate_interval: f32,
    
    /// graphics refresh rate in Hz
    #[arg(long, default_value_t = 10.0)]
    refresh_rate: f32,

    #[arg(long, default_value_t = 25.0)]
    count_rate_threshold: f32,
}

fn main() {
    let args = Args::parse();

    let empty_hist = PointHistogram::new(
        args.hist_min..args.hist_max,
        args.hist_bins,
    );

    let histogram_bg = Arc::new(Mutex::new(empty_hist.clone()));
    let histogram = Arc::clone(&histogram_bg);

    let channels_bg = Arc::new(Mutex::new(BTreeMap::new()));
    let channels = Arc::clone(&channels_bg);

    let count_rate_bg = Arc::new(Mutex::new(BTreeMap::new()));
    let count_rate = Arc::clone(&count_rate_bg);

    std::thread::spawn(move || {
        let tqdc_address = SocketAddr::new(args.tqdc_ip, TQDC_STREAM_PORT);

        let sock = UdpSocket::bind("0.0.0.0:0").unwrap();
        
        sock.send_to(
            &MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
                0x0000, 
                0x0001,
                0xfefe,
                0xFFFF, 
                0xFFFF,
            )),
            tqdc_address,
        )
        .unwrap();

        let mut count_rate_timer = Instant::now();
        let mut plots_refresh_timer = Instant::now();
        
        let mut counts = BTreeMap::new();
        let mut channels = BTreeMap::new();
        let mut histogram = empty_hist.clone();

        let mut fragments_buf: BTreeMap<u16, Vec<tqdc::mstream::MStreamFragment>> = BTreeMap::new();
        
        let mut seq = 0;

        loop {
            let mut buf = [0; MTU_SIZE];
            let (len, to_addr) = sock.recv_from(&mut buf).unwrap();
            let message = MlinkMessage::from_datagram(&buf[..len]);

            let count_rate_interval_ms = (args.count_rate_interval * 1000.0) as u128;
            let plots_refresh_interval_ms = ( 1.0 / args.refresh_rate * 1000.0) as u128;

            match message {
                MlinkMessage::StreamReq { fragment, ..} => {
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
                    seq +=1;

                    fragments_buf.entry(fragment.header.fragment_id).or_insert(vec![]).push(fragment);

                    let elapsed_ms = count_rate_timer.elapsed().as_millis();
                    if elapsed_ms > count_rate_interval_ms {
                        count_rate_timer = Instant::now();
                        counts.iter_mut().for_each(|(_, count)| {
                            *count /= (elapsed_ms) as f32 / 1000.0;
                        });
                        *count_rate_bg.lock().unwrap() = counts.clone(); // TODO: fix count rate calculation
                        counts.clear();
                    }

                    let elapsed_ms = plots_refresh_timer.elapsed().as_millis();
                    if elapsed_ms > plots_refresh_interval_ms {
                        fragments_buf.iter().for_each(|(_, fragments)| {
                            let fragments = fragments.iter().collect::<Vec<_>>();
                            if let Ok(combined) = MStreamTriggerAndUserData::try_from(&fragments[..]) {
                                channels = combined.extract_data_blocks().into_iter().map(|ADCDataBlock {ch_num, waveform }| {
                                    let waveform = process_waveform(waveform);
                                    waveform_to_events(&waveform, &Algorithm::Max).into_iter().for_each(|(_, amp)| {
                                        if amp > args.count_rate_threshold {
                                            *counts.entry(ch_num).or_insert(0.0) += 1.0;
                                        }
                                        histogram.add(ch_num, amp)
                                    });
                                    (ch_num, waveform)
                                }).collect::<BTreeMap<_, _>>();
                            }
                        });
                        fragments_buf.clear();

                        plots_refresh_timer = Instant::now();
                        *channels_bg.lock().unwrap() = channels.clone();
                        channels.clear();
                        *histogram_bg.lock().unwrap() = histogram.clone();
                    }
                }
                _ => panic!("unimplemented!!"),
            }
        }
    });

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "My egui App",
        native_options,
        Box::new(|_| {
            Box::new(MyEguiApp {
                channels,
                histogram,
                count_rate,
            })
        }),
    ).unwrap();
}

struct MyEguiApp {
    histogram: Arc<Mutex<PointHistogram>>,
    channels: Arc<Mutex<BTreeMap<u8, ProcessedWaveform>>>,
    count_rate: Arc<Mutex<BTreeMap<u8, f32>>>,
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(1000 / 60));

        let channels = self.channels.lock().unwrap().clone();

        egui::TopBottomPanel::top("count_rates").show(ctx, |ui| {
            let count_rate_lock = self.count_rate.lock().unwrap();

            ui.label("count_rates:");
            for (ch_num, count_rate) in count_rate_lock.iter() {
                ui.label(format!("ch {}: {count_rate:>8.2} Hz", ch_num + 1));
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                ui.vertical(|ui| {
                    ui.set_width(500.0);
                    Plot::new("waveforms")
                        .legend(Legend {
                            text_style: egui::TextStyle::Body,
                            background_alpha: 1.0,
                            position: egui::plot::Corner::RightTop,
                        })
                        .show(ui, |plot_ui| {
                            channels.into_iter().for_each(|(ch_num, waveform)| {
                                waveform.draw_egui(
                                    plot_ui, 
                                    Some(&format!("ch #{}", ch_num + 1)), 
                                    Some(color_for_index(ch_num as usize)), 
                                    None, 
                                    None
                                );
                            });
                        });
                });
                ui.vertical(|ui| {
                    Plot::new("hists")
                        .legend(Legend {
                            text_style: egui::TextStyle::Body,
                            background_alpha: 1.0,
                            position: egui::plot::Corner::RightTop,
                        })
                        .show(ui, |plot_ui| {
                            let hist = self.histogram.lock().unwrap().clone();
                            hist.draw_egui_each_channel(plot_ui, None);
                        });
                });
            })
        });
    }
}

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use eframe::egui;
use egui_plot::{Legend, Plot};
use egui::mutex::Mutex;

use apps::defaults::BOARD_IP;
use processing::process::{frame_to_events, TRAPEZOID_DEFAULT};
use processing::types::{FrameEvent, NumassFrameFast, ProcessedWaveform};
use processing::utils::{color_for_index, EguiLine};
use processing::histogram::PointHistogram;
use tqdc::{MTU_SIZE, TQDC_STREAM_PORT};
use tqdc::mlink::MlinkMessage;
use tqdc::mstream::{MStreamTriggerAndUserData, ADCDataBlock};

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

    #[arg(long, default_value_t = 400)]
    hist_bins: usize,
    
    /// graphics refresh rate in Hz
    #[arg(long, default_value_t = 1.0)]
    refresh_rate: f32,

    #[arg(long, default_value_t = 25.0)]
    count_rate_threshold: f32,
}

fn main() {
    let args = Args::parse();

    let histogram_bg = Arc::new(Mutex::new(PointHistogram::new(
        args.hist_min..args.hist_max,
        args.hist_bins,
    )));

    let histogram = Arc::clone(&histogram_bg);

    let channels_bg = Arc::new(Mutex::new(BTreeMap::new()));
    let channels = Arc::clone(&channels_bg);

    let count_rate_bg = Arc::new(Mutex::new(BTreeMap::new()));
    let count_rate = Arc::clone(&count_rate_bg);

    std::thread::spawn(move || {
        let tqdc_address = SocketAddr::new(args.tqdc_ip, TQDC_STREAM_PORT);

        let sock = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        
        sock.send_to(
            &MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
                0x0000, 
                0x0001,
                0xfefe,
                0xFFFF, 
                0xFFFF,
            )),
            tqdc_address,
        ).unwrap();

        let mut refresh_timer = Instant::now();
        
        let counts = Arc::new(Mutex::new(BTreeMap::new()));
        
        let mut fragments_buf: BTreeMap<u16, BTreeMap<u16, tqdc::mstream::MStreamFragment>> = BTreeMap::new();
        
        let mut seq = 0;

        loop {
            let mut buf = [0; MTU_SIZE];
            let (len, to_addr) = sock.recv_from(&mut buf).unwrap();
            let message = MlinkMessage::from_datagram(&buf[..len]);

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
                    ).unwrap();
                    seq +=1;

                    fragments_buf.entry(fragment.header.fragment_id).or_default().insert(
                        fragment.header.fragment_offset, fragment
                    );

                    let elapsed_ms = refresh_timer.elapsed().as_millis();
                    if elapsed_ms > plots_refresh_interval_ms {

                        let channels_bg = Arc::clone(&channels_bg);
                        let histogram_bg = Arc::clone(&histogram_bg);
                        let counts = Arc::clone(&counts);

                        let count_rate_bg = Arc::clone(&count_rate_bg);

                        fragments_buf = {

                            std::thread::spawn(move || {
                                let mut counts = counts.lock();
                                let mut histogram = histogram_bg.lock();


                                let mut channels = None;
    
                                fragments_buf.iter().for_each(|(_, fragments)| {
                                    let fragments = fragments.values().collect::<Vec<_>>();
                                    if let Ok(combined) = MStreamTriggerAndUserData::try_from(&fragments[..]) {

                                        let data_blocks = combined.extract_data_blocks();
                                        let frame: NumassFrameFast = data_blocks.iter().map(|ADCDataBlock {ch_num, waveform }| {
                                            (*ch_num, waveform.as_slice())
                                        }).collect::<BTreeMap<_,_>>();

                                        let events = frame_to_events(
                                            &frame, 
                                            &TRAPEZOID_DEFAULT, 
                                            None, 
                                            &mut None
                                        );

                                        events.into_iter().for_each(|(_, event)| {
                                            if let FrameEvent::Event { channel, amplitude, .. } = event {
                                                if amplitude > args.count_rate_threshold {
                                                    *counts.entry(channel).or_insert(0.0f64) += 1.0;
                                                }
                                                histogram.add(channel, amplitude);
                                            }
                                        });

                                        if channels.is_none() {
                                            channels = Some(frame.iter().map(|(ch_num, waveform)| {
                                                (*ch_num, ProcessedWaveform::from(*waveform))
                                            }).collect::<BTreeMap<_,_>>());
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
    count_rate: Arc<Mutex<BTreeMap<u8, f64>>>,
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(1000 / 60));

        let channels = self.channels.lock().clone();

        egui::TopBottomPanel::top("count_rates").show(ctx, |ui| {
            let count_rate_lock = self.count_rate.lock();

            ui.label("count_rates:");
            for (ch_num, count_rate) in count_rate_lock.iter() {
                ui.label(format!("ch {}: {count_rate:>8.2} Hz", ch_num + 1));
            }
        });

        egui::SidePanel::left("waveforms").default_width(500.0).show(ctx, |ui| {
            Plot::new("waveforms").legend(Legend::default())
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

        egui::CentralPanel::default().show(ctx, |ui| {
            Plot::new("hists").legend(Legend::default())
                .show(ui, |plot_ui| {
                    let hist = self.histogram.lock().clone();
                    hist.draw_egui_each_channel(plot_ui, None);
                });
        });
    }
}

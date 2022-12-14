use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::net::{UdpSocket, SocketAddr};
use std::time::Instant;

use eframe::egui;
use eframe::egui::plot::{Plot, Line, Legend};
use clap::Parser;

use tqdc::mlink::MlinkMessage;
use apps::PointHistogramm;
use apps::defaults::{HOST_IP, BOARD_IP, STREAM_PORT, HOST_STREAM_PORT};



/// Read TQDC register that contains id (programm does not have timeout)
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
   #[arg(long, default_value_t = HOST_IP)]
   host_ip: std::net::IpAddr,

   #[arg(long, default_value_t = HOST_STREAM_PORT)]
   host_stream_port: u16,

   #[arg(long, default_value_t = BOARD_IP)]
   tqdc_ip: std::net::IpAddr,

   #[arg(long, default_value_t = STREAM_PORT)]
   tqdc_stream_port: u16,

   #[arg(long, default_value_t = 0)]
   hist_min: i32,

   #[arg(long, default_value_t = 400)]
   hist_max: i32,

   #[arg(long, default_value_t = 400)]
   hist_bins: usize,

   #[arg(long, default_value_t = 3)]
   count_rate_interval_sec: u128,

   #[arg(long, default_value_t = 25)]
   count_rate_threshold: i16,
}

fn main() {

    let args = Args::parse();

    let histogram_bg = Arc::new(
        Mutex::new(
            PointHistogramm::new((args.hist_min, args.hist_max), args.hist_bins)));
    let histogram = Arc::clone(&histogram_bg);

    
    let waveforms_bg = Arc::new(Mutex::new(vec![]));
    let waveforms = Arc::clone(&waveforms_bg);

    let count_rate_bg = Arc::new(Mutex::new(HashMap::new()));
    let count_rate = Arc::clone(&count_rate_bg);

    std::thread::spawn(move || {

            let bind_address = SocketAddr::new(args.host_ip, args.host_stream_port);
            let tqdc_address = SocketAddr::new(args.tqdc_ip, args.tqdc_stream_port);
        
            let sock = UdpSocket::bind(bind_address).unwrap();

            sock.send_to(&MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
                0x0000, 
                0x0101, 
                0x0001, 
                0xFFFF, 
                0xFFFF
            )), tqdc_address).unwrap();


            let mut start = Instant::now();
            let mut counts = HashMap::new();

            loop {
                let mut buf = [0; 4096 * 10];
                let (len, to_addr) = sock.recv_from(&mut buf).unwrap();
                let message = MlinkMessage::from_datagram(&buf[..len]);

                match message {
                    MlinkMessage::StreamReq { header, frames } => {
                        if let Some(frame) = frames.first() {

                            sock.send_to(&MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
                                header.seq, 
                                0x0101, 
                                0x0001, 
                                frame.fragment_offset, 
                                frame.fragment_id, 
                            )), to_addr).unwrap();

                            let channels = frames.iter().flat_map(|fragment| {
                                fragment.channels.iter().map(|channel| {

                                    let first = channel.waveform.iter().take(16).sum::<i16>() / 16;

                                    Waveform {
                                        ch_num: channel.channel_number, 
                                        waveform: channel.waveform.iter().map(|v| (v - first) / 4).collect::<Vec<_>>()
                                    }
                                })
                            }).collect::<Vec<_>>();


                            {
                                for Waveform {ch_num, waveform} in &channels {
                                    let amplitude = waveform.iter().max().unwrap();
                                    if amplitude > &args.count_rate_threshold {
                                        *counts.entry(*ch_num).or_insert(0u16) += 1;
                                    }
                                }
                            }

                            let elapsed_ms = start.elapsed().as_millis();
                            if elapsed_ms > args.count_rate_interval_sec * 1000 {

                                let mut count_rate_lock = count_rate_bg.lock().unwrap();
                                *count_rate_lock = counts.iter()
                                    .map(|(ch_num, counts)| (*ch_num, ((*counts as f32) / (elapsed_ms as f32 / 1000.0)) as u32))
                                    .collect::<HashMap<_,_>>();

                                start = Instant::now();
                                counts = HashMap::new();
                            }

                            {
                                let mut hist_lock = histogram_bg.lock().unwrap();
                                for Waveform {ch_num, waveform} in &channels {
                                    let amplitude = *waveform.iter().max().unwrap();
                                    hist_lock.add(*ch_num, amplitude); 
                                }
                            }

                            {
                                let mut waveform_lock = waveforms_bg.lock().unwrap();
                                *waveform_lock = channels;
                            }

                        } else {
                            panic!("no frames in package!")
                        }
                    }
                    _ => panic!("unimplemented!!")
                }
            }
    });

    let native_options = eframe::NativeOptions::default();
    eframe::run_native("My egui App", native_options, Box::new(|_| Box::new(MyEguiApp {
        waveforms,
        histogram,
        count_rate
    })));
}

#[derive(Clone)]
struct Waveform {
    ch_num: u8,
    waveform: Vec<i16>
}

struct MyEguiApp {
    waveforms: Arc<Mutex<Vec<Waveform>>>,
    histogram: Arc<Mutex<PointHistogramm>>,
    count_rate: Arc<Mutex<HashMap<u8, u32>>>
}

impl eframe::App for MyEguiApp {
   fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {

       ctx.request_repaint_after(std::time::Duration::from_millis(1000/60));

    //    println!("{:?}", ctx.used_size());

        let channels = {
            let lock = self.waveforms.lock();
            let mut channels_map = HashMap::new();
            for Waveform {ch_num, waveform} in lock.unwrap().clone() {
                channels_map.insert(ch_num, waveform);
            }
            channels_map
        };

        egui::TopBottomPanel::top("count_rates").show(ctx, |ui| {
            let count_rate_lock = self.count_rate.lock().unwrap();

            let mut channels = Vec::from_iter(count_rate_lock.iter());
            channels.sort_by_key(|(ch_num, _)| **ch_num);

            ui.label("count_rates:");
            for (ch_num, count_rate) in channels {
                ui.label(format!("ch {}: {count_rate: >8} Hz", ch_num + 1));
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {

            let mut sorted = channels.iter().collect::<Vec<_>>();
            sorted.sort_by_key(|k| k.0);

            let lines = sorted.iter().map(|(ch_num, waveform)| {
                Line::new(
                    waveform.iter().enumerate().map(|(x, y)| [x as f64, *y as f64]).collect::<Vec<_>>()).name(
                        format!("ch #{}", **ch_num + 1)
                )
            });

            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                ui.vertical(|ui| {
                    ui.set_width(500.0);
                    Plot::new("waveforms").legend(Legend { 
                        text_style: egui::TextStyle::Body, 
                        background_alpha: 1.0, position: egui::plot::Corner::RightTop 
                    }).show(ui, |plot_ui| {
                        lines.for_each(|line| {
                            plot_ui.line(line)
                        })
                    });
                });
                ui.vertical(|ui| {
                    Plot::new("hists").legend(Legend { 
                        text_style: egui::TextStyle::Body, 
                        background_alpha: 1.0, position: egui::plot::Corner::RightTop 
                        }).show(ui, |plot_ui| {
                            let hist = self.histogram.lock().unwrap().clone();
                            let mut channels = Vec::from_iter(hist.channels.iter());
                            channels.sort_by_key(|(ch_num, _)| **ch_num);
                            for (ch_num, y) in channels {
                                plot_ui.line(Line::new(
                                y.iter().enumerate().flat_map(|(x, y)| [
                                    [(hist.x[x] - hist.step / 2.0)  as f64, *y as f64],
                                    [(hist.x[x] + hist.step / 2.0)  as f64, *y as f64]
                                ]).collect::<Vec<_>>()).name(
                                    format!("ch #{}", ch_num + 1)
                                ));
                            }
                        });
                });
            })
        });
   }
}
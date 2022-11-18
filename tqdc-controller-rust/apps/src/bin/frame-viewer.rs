use std::sync::Arc;
use std::net::{UdpSocket, SocketAddr};

use eframe::egui;
use eframe::egui::mutex::Mutex;
use eframe::egui::plot::{Plot, Line};
use clap::Parser;

use tqdc::mlink::MlinkMessage;
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
}

fn main() {

    let args = Args::parse();

    let native_options = eframe::NativeOptions::default();

    let v = Arc::new(Mutex::new(vec![]));
    let v2 = Arc::clone(&v);

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
                                    (channel.channel_number, channel.waveform.clone())
                                })
                            }).collect::<Vec<_>>();

                            let mut lock = v.lock();
                            *lock = channels;

                        } else {
                            panic!("no frames in package!")
                        }
                    }
                    _ => panic!("unimplemented!!")
                }
            }
    });

    eframe::run_native("My egui App", native_options, Box::new(|_| Box::new(MyEguiApp {
        value: v2
    })));
}

struct MyEguiApp {
    value: Arc<Mutex<Vec<(u8, Vec<i16>)>>>
}


impl eframe::App for MyEguiApp {
   fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {

       ctx.request_repaint_after(
        std::time::Duration::from_millis(50));
       egui::CentralPanel::default().show(ctx, |ui| {
            //    ui.heading(format!("{:?}", *lock));

            let channels = {
                let lock = self.value.lock();
                lock.clone()
            };

            let lines =  channels.iter().map(|(ch_num, waveform)| {
                Line::new(
                    waveform.iter().enumerate().map(|(x, y)| [x as f64, *y as f64]).collect::<Vec<_>>()).name(
                        format!("{ch_num}")
                    )
            });

            Plot::new("Test Plot").show(ui, |plot_ui| {
                lines.for_each(|line| {
                    plot_ui.line(line)
                })
            });
       });
   }
}
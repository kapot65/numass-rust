use eframe::egui;
use std::sync::Arc;
use std::net::UdpSocket;

use egui::mutex::Mutex;
use egui::plot::{Plot, Line};
use tqdc::mlink::MlinkMessage;

fn main() {
    let native_options = eframe::NativeOptions::default();

    let v = Arc::new(Mutex::new(vec![]));
    let v2 = Arc::clone(&v);

    std::thread::spawn(move || {
        
            let sock = UdpSocket::bind("0.0.0.0:33301").unwrap();
            let to_addr = "10.0.0.5:33301";

            sock.send_to(&MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
                0x0000, 
                0x0101, 
                0x0001, 
                0xFFFF, 
                0xFFFF
            )), to_addr).unwrap();


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

       ctx.request_repaint_after(std::time::Duration::from_millis(50));
       egui::CentralPanel::default().show(ctx, |ui| {
            //    ui.heading(format!("{:?}", *lock));

           let channels = {
                let lock = self.value.lock();
                lock.clone()
           };
           
           let lines =  channels.iter().map(|(_, waveform)| {
                Line::new(
                    waveform.iter().enumerate().map(|(x, y)| [x as f64, *y as f64]).collect::<Vec<_>>())
           });

            Plot::new("Test Plot").show(ui, |plot_ui| {
                lines.for_each(|line| {
                    plot_ui.line(line)
                })
            });
       });
   }
}
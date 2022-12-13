use protobuf::Message;
use eframe::{egui, epaint::Color32};
use clap::Parser;

use dataforge::protos::rsb_event;


#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Opt {
    directory: std::path::PathBuf,
}


#[tokio::main]
async fn main() {

    let args = Opt::parse();

    let mut point_file = tokio::fs::File::open(args.directory).await.unwrap();
    let message = dataforge::extract_df_message(&mut point_file).await.unwrap();

    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

    let limit_ns = 1_000_000;


    let mut chunks = vec![];
    chunks.push(vec![]);

    for channel in point.channels {
        for block in channel.blocks {
            for frame in block.frames {

                let chunk_num = (frame.time / (limit_ns as u64)) as usize;

                while chunks.len() < chunk_num + 1 {
                    chunks.push(vec![])
                }

                let waveform_len = frame.data.len() / 2;
                    let waveform = (0..waveform_len).map(|idx| {
                        let x = (frame.time + 8u64 * (idx as u64) - (chunk_num as u64 * limit_ns)) as f64;
                        let y = i16::from_le_bytes(frame.data[idx*2..idx*2+2].try_into().unwrap()) as f64;
                        [x / 1000.0, y]
                    }).collect::<Vec<_>>();
                    chunks[chunk_num].push((channel.id as u8, waveform))
            }
        }
    }

    let native_options = eframe::NativeOptions::default();
    eframe::run_native("My egui App", native_options, Box::new(|_| Box::new(MyEguiApp {
        chunks,
        current_chunk: 0
    })));
}


struct MyEguiApp {
    chunks: Vec<Vec<(u8, Vec<[f64; 2]>)>>,
    current_chunk: usize
}

impl eframe::App for MyEguiApp {
   fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {

        if ctx.input().key_pressed(egui::Key::ArrowRight) {
            self.current_chunk += 1;
        }

        if ctx.input().key_pressed(egui::Key::ArrowLeft) {
            self.current_chunk -= 1;
        }


        let colors = [
            Color32::RED, 
            Color32::BLUE, 
            Color32::GREEN, 
            Color32::YELLOW, 
            Color32::WHITE,
            Color32::GOLD,
            Color32::KHAKI
        ];

        egui::CentralPanel::default().show(ctx, |ui| {

            ui.style_mut().spacing.slider_width = 500.0;

            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.current_chunk, 0..=self.chunks.len() - 1).step_by(1.0));
                if ui.button("-").clicked() {
                    self.current_chunk -= 1;
                }
                if ui.button("+").clicked() {
                    self.current_chunk += 1;
                }
            });

            egui::plot::Plot::new("waveforms").show(ui, |plot_ui| {

                for (ch_num, x) in self.chunks[self.current_chunk].clone() {
                    plot_ui.line(
                        egui::plot::Line::new(x).color(colors[(ch_num)as usize]));
                }
            });
        });
   }
}
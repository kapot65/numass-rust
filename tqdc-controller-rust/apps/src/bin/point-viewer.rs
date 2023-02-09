#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] 

use processing::utils::channel_colors;
use protobuf::Message;
use eframe::{egui, epaint::Color32};
use clap::Parser;

use dataforge::read_df_message;
use numass::{protos::rsb_event, NumassMeta};


#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Opt {
    filepath: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() {

    let args = Opt::parse();

    let filepath = args.filepath.unwrap_or_else(|| {
        rfd::FileDialog::new().pick_file().expect("no file choosen")
    });

    let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
    let message = read_df_message::<NumassMeta>(&mut point_file).await.unwrap();

    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

    let limit_ns = 1_000_000;


    let mut chunks = vec![];
    chunks.push(vec![]);

    for channel in point.channels {
        for block in channel.blocks {
            for frame in block.frames {

                let chunk_num = (frame.time / limit_ns) as usize;

                while chunks.len() < chunk_num + 1 {
                    chunks.push(vec![])
                }

                // TODO: Refactor with processing::frame_to_waveform
                let waveform_len = frame.data.len() / 2;
                let waveform = (0..waveform_len).map(|idx| {
                    let x = (frame.time + 8u64 * (idx as u64) - (chunk_num as u64 * limit_ns)) as f64;
                    let y = i16::from_le_bytes(frame.data[idx*2..idx*2+2].try_into().unwrap()) as f64;
                    [x / 1000.0, y]
                });

                let baseline = waveform.clone().take(16).map(|[_, y]| y).sum::<f64>() / 16.0;
                chunks[chunk_num].push((channel.id as u8, waveform.map(|[x,y]| [x, y - baseline]).collect::<Vec<_>>()))
            }
        }
    }

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(std::fs::canonicalize(&filepath).unwrap().to_str().unwrap(), native_options, Box::new(|_| Box::new(PointViewer {
        chunks,
        current_chunk: 0,
        colors: channel_colors()
    })));
}


struct PointViewer {
    chunks: Vec<Vec<(u8, Vec<[f64; 2]>)>>,
    current_chunk: usize,
    colors: [Color32; 7]
}

impl eframe::App for PointViewer {
   fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {    

        if ctx.input().key_pressed(egui::Key::ArrowRight) && self.current_chunk < self.chunks.len() - 1 {
            self.current_chunk += 1;
        }

        if ctx.input().key_pressed(egui::Key::ArrowLeft) && self.current_chunk > 0 {
            self.current_chunk -= 1;
        }

        egui::CentralPanel::default().show(ctx, |ui| {

            ui.style_mut().spacing.slider_width = frame.info().window_info.size.x - 150.0;

            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.current_chunk, 0..=self.chunks.len() - 1)
                    .suffix(" ms")
                    .step_by(1.0));
                if ui.button("<").clicked() && self.current_chunk > 0 {
                    self.current_chunk -= 1;
                }
                if ui.button(">").clicked() && self.current_chunk < self.chunks.len() - 1 {
                    self.current_chunk += 1;
                }
            });

            egui::plot::Plot::new("waveforms")
                .legend(egui::plot::Legend { 
                    text_style: egui::TextStyle::Body, 
                    background_alpha: 1.0, position: egui::plot::Corner::RightTop 
                })
                .x_axis_formatter(|value, _| {
                    format!("{value:.3} μs")
                })
                .show(ui, |plot_ui| {

                    for (ch_num, x) in self.chunks[self.current_chunk].clone() {
                        plot_ui.line(
                            egui::plot::Line::new(x).color(self.colors[(ch_num)as usize]).name(format!("ch #{}", ch_num + 1)));
                    }
                });
        });
   }
}
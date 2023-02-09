#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] 

use std::collections::BTreeMap;

use clap::Parser;

use eframe::epaint::Color32;
use protobuf::Message;

use dataforge::read_df_message;
use processing::{frame_to_waveform, waveform_to_event, convert_to_kev, utils::channel_colors};
use numass::{protos::rsb_event, NumassMeta};


#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Opt {
    filepath: std::path::PathBuf,
    #[clap(long, default_value_t = 0.0)]
    min: f32,
    #[clap(long, default_value_t = 5.0)]
    max: f32
}

#[tokio::main]
async fn main() {

    let args = Opt::parse();

    let filepath = args.filepath;

    let range = args.min..args.max;

    let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
    let message = read_df_message::<NumassMeta>(&mut point_file).await.unwrap();

    let mut independent: BTreeMap<u64, BTreeMap<u8, Vec<i16>>> = BTreeMap::new();

    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
    for channel in &point.channels {
        for block in &channel.blocks {
            for frame in &block.frames {
                let entry = independent.entry(frame.time).or_default();
                entry.insert(channel.id as u8, frame_to_waveform(frame));
            }
        }
    }

    let algorithm = processing::Algorithm::Likhovid { left: 6, right: 36 };

    let independent = independent.iter().filter_map(|(time, waveforms)| {
        if !waveforms.iter().map(|(ch_id, waveform)| {
            let (_, amp) = waveform_to_event(waveform, &algorithm);
            convert_to_kev(&amp, *ch_id, &algorithm)
        }).any(|amp| range.contains(&amp)) {
            return None;
        }
        Some((*time, waveforms.clone()))
    }).collect::<Vec<_>>();


    let native_options = eframe::NativeOptions::default();
    eframe::run_native(format!("filtered {filepath:?} ({range:?} keV)").as_str(), native_options, Box::new(|_| Box::new(FilteredViewer {
        independent,
        current: 0,
        colors: channel_colors()
    })));

}

struct FilteredViewer {
    independent: Vec<(u64, BTreeMap<u8, Vec<i16>>)>,
    current: usize,
    colors: [Color32; 7]
}

impl eframe::App for FilteredViewer {
   fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {    

        if ctx.input().key_pressed(eframe::egui::Key::ArrowRight) && self.current < self.independent.len() - 1 {
            self.current += 1;
        }

        if ctx.input().key_pressed(eframe::egui::Key::ArrowLeft) && self.current > 0 {
            self.current -= 1;
        }

        eframe::egui::CentralPanel::default().show(ctx, |ui| {

            ui.style_mut().spacing.slider_width = frame.info().window_info.size.x - 200.0;
            
            ui.horizontal(|ui| {
                ui.add(eframe::egui::Slider::new(&mut self.current, 0..=self.independent.len() - 1)
                    .step_by(1.0));
                if ui.button("<").clicked() && self.current > 0 {
                    self.current -= 1;
                }
                if ui.button(">").clicked() && self.current < self.independent.len() - 1 {
                    self.current += 1;
                }

                ui.label(format!("{:.3} ms", self.independent[self.current].0 as f64 / 1e6))
            });

            eframe::egui::plot::Plot::new("waveforms")
                .legend(eframe::egui::plot::Legend { 
                    text_style: eframe::egui::TextStyle::Body, 
                    background_alpha: 1.0, position: eframe::egui::plot::Corner::RightTop 
                })
                .x_axis_formatter(|value, _| {
                    format!("{:.3} μs", (value * 8.0) / 1000.0)
                })
                .show(ui, |plot_ui| {
                    for (ch_id, waveform) in &self.independent[self.current].1 {
                        plot_ui.line(
                            eframe::egui::plot::Line::new(waveform.iter().enumerate().map(|(x, y)| {
                                [x as f64, *y as f64]
                            }).collect::<Vec<_>>()).color(self.colors[*ch_id as usize]).name(format!("ch# {}", ch_id + 1)));
                    };
                });
        });
   }
}
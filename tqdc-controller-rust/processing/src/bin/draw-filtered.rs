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
    max: f32,
    #[clap(long, default_value_t = 5000)]
    neigborhood: u64
}

#[derive(Debug, Clone)]
struct DeviceFrame {
    time: u64,
    waveforms: BTreeMap<u8, Vec<i16>>
}

#[tokio::main]
async fn main() {

    let args = Opt::parse();

    let filepath = args.filepath;

    let range = args.min..args.max;

    let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
    let message = read_df_message::<NumassMeta>(&mut point_file).await.unwrap();

    let mut events: BTreeMap<u64, BTreeMap<u8, Vec<i16>>> = BTreeMap::new();

    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
    for channel in &point.channels {
        for block in &channel.blocks {
            for frame in &block.frames {
                let entry = events.entry(frame.time).or_default();
                entry.insert(channel.id as u8, frame_to_waveform(frame));
            }
        }
    }

    let algorithm = processing::Algorithm::Likhovid { left: 6, right: 36 };

    let events = events.iter().map(|(time, waveforms)| (*time, waveforms.clone())).collect::<Vec<_>>();

    let independent = events.iter().enumerate().filter_map(|(idx, (time, waveforms))| {
        if !waveforms.iter().map(|(ch_id, waveform)| {
            let (_, amp) = waveform_to_event(waveform, &algorithm);
            convert_to_kev(&amp, *ch_id, &algorithm)
        }).any(|amp| range.contains(&amp)) {
            return None;
        }

        let neighbors = {
            let bounds = (*time - args.neigborhood)..(*time + args.neigborhood);
            let mut neighbors = vec![];
            let mut left = idx;
            loop {
                if left == 0 { break; }
                if !bounds.contains(&events[left - 1].0) { break; }
                neighbors.push(DeviceFrame {
                    time: events[left - 1].0, 
                    waveforms: events[left - 1].1.clone()
                });
                left -= 1;
            }
            let mut right = idx;
            loop {
                if right == events.len() - 1 { break; }
                if !bounds.contains(&events[right + 1].0) { break; }
                neighbors.push(DeviceFrame {
                    time: events[right + 1].0, 
                    waveforms: events[right + 1].1.clone()
                });
                right += 1;
            }
            neighbors
        };
        Some((DeviceFrame {
            time: *time, 
            waveforms: waveforms.clone()
        }, neighbors))
    }).collect::<Vec<_>>();


    let native_options = eframe::NativeOptions::default();
    eframe::run_native(format!("filtered {filepath:?} ({range:?} keV)").as_str(), native_options, Box::new(|_| Box::new(FilteredViewer {
        independent,
        current: 0,
        colors: channel_colors()
    })));
}

struct FilteredViewer {
    independent: Vec<(DeviceFrame, Vec<DeviceFrame>)>,
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

        let (current, neighbors) = &self.independent[self.current];

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

                ui.label(format!("{:.3} ms", current.time as f64 / 1e6))
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


                    for (ch_id, waveform) in &current.waveforms {
                        plot_ui.line(
                            eframe::egui::plot::Line::new(waveform.iter().enumerate().map(|(x, y)| {
                                [x as f64, *y as f64]
                            }).collect::<Vec<_>>())
                            .width(3.0)
                            .color(self.colors[*ch_id as usize])
                            .name(format!("ch# {}", ch_id + 1)));
                    };

                    neighbors.iter().for_each(|DeviceFrame { time, waveforms }| {

                        let delta = (*time as f64 - current.time as f64) / 8.0;

                        for (ch_id, waveform) in waveforms {
                            plot_ui.line(
                                eframe::egui::plot::Line::new(waveform.iter().enumerate().map(|(x, y)| {
                                    [x as f64 + delta, *y as f64]
                                }).collect::<Vec<_>>())
                                .width(1.0)
                                .color(self.colors[*ch_id as usize])
                                .name(format!("ch# {}", ch_id + 1)));
                        };
                    });
                });
        });
   }
}
use std::collections::BTreeMap;

use processing::{frame_to_waveform, waveform_to_event, convert_to_kev};
use protobuf::Message;

use dataforge::read_df_message;
use numass::{protos::rsb_event, NumassMeta};

#[tokio::main]
async fn main() {

    // let filepath = "/data/numass-server/2022_12/Tritium_7/set_1/p120(30s)(HV1=12000)";
    let filepath = "/data/numass-server/2022_12/Adiabacity_19_2/set_1/p6(200s)(HV1=13000)";
    // let filepath = "/data/numass-server/2022_12/Gun_19/set_1/p0(200s)(HV1=18990)";

    let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
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

    let independent = independent.iter()
    .filter(|(_, waveforms)| { 
        if !(waveforms.len() == 1 && waveforms.contains_key(&5)) {
            return false;
        }

        let (_, amp) = waveform_to_event(&waveforms[&5], &algorithm);

        let amp_kev = convert_to_kev(&amp, 5, &algorithm);

        (12.0..13.0).contains(&amp_kev)
    }).map(|(_, waveform)| waveform[&5].clone()).collect::<Vec<_>>();


    let native_options = eframe::NativeOptions::default();
    eframe::run_native("filtered", native_options, Box::new(|_| Box::new(FilteredViewer {
        independent,
        current: 0
    })));

}

struct FilteredViewer {
    independent: Vec<Vec<i16>>,
    current: usize,
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

            ui.style_mut().spacing.slider_width = frame.info().window_info.size.x - 150.0;
            
            ui.horizontal(|ui| {
                ui.add(eframe::egui::Slider::new(&mut self.current, 0..=self.independent.len() - 1)
                    .step_by(1.0));
                if ui.button("<").clicked() && self.current > 0 {
                    self.current -= 1;
                }
                if ui.button(">").clicked() && self.current < self.independent.len() - 1 {
                    self.current += 1;
                }
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

                    let waveform = &self.independent[self.current];
                    plot_ui.line(
                        eframe::egui::plot::Line::new(waveform.iter().enumerate().map(|(x, y)| {
                            [x as f64, *y as f64]
                        }).collect::<Vec<_>>()));
                });
        });
   }
}
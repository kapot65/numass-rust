use std::{collections::BTreeMap, path::PathBuf};

use processing::{
    process::process_waveform, 
    storage::load_point, 
    types::ProcessedWaveform, 
    utils::{color_for_index, EguiLine}
};


#[tokio::main]
async fn main() {
    

    // let files = [
    //     "/data/numass-server/2022_12/Tritium_7/set_1/p52(30s)(HV1=15000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_2/p52(30s)(HV1=15000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_3/p52(30s)(HV1=15000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_4/p52(30s)(HV1=15000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_5/p52(30s)(HV1=15000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_6/p52(30s)(HV1=15000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_7/p52(30s)(HV1=15000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_8/p52(30s)(HV1=15000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_9/p52(30s)(HV1=15000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_10/p52(30s)(HV1=15000)",
    // ];

    let files = [
        "/data/numass-server/2022_12/Tritium_7/set_1/p64(30s)(HV1=14500)",
        "/data/numass-server/2022_12/Tritium_7/set_2/p64(30s)(HV1=14500)",
        "/data/numass-server/2022_12/Tritium_7/set_3/p64(30s)(HV1=14500)",
        "/data/numass-server/2022_12/Tritium_7/set_4/p64(30s)(HV1=14500)",
        "/data/numass-server/2022_12/Tritium_7/set_5/p64(30s)(HV1=14500)",
        "/data/numass-server/2022_12/Tritium_7/set_6/p64(30s)(HV1=14500)",
        "/data/numass-server/2022_12/Tritium_7/set_7/p64(30s)(HV1=14500)",
        "/data/numass-server/2022_12/Tritium_7/set_8/p64(30s)(HV1=14500)",
        "/data/numass-server/2022_12/Tritium_7/set_9/p64(30s)(HV1=14500)",
        "/data/numass-server/2022_12/Tritium_7/set_10/p64(30s)(HV1=14500)",
    ];

    // let monitor_indices = [30,39,47,53,58,66,75,76,85,93,103,111,121];
    // let mut files = vec![];
    // for set_number in 1..=10 {
    //     for point_idx in monitor_indices {
    //         files.push(format!("/data/numass-server/2022_12/Tritium_7/set_{set_number}/p{point_idx}(30s)(HV1=14000)"))
    //     }
    // }

    // let files = [
    //     "/data/numass-server/2022_12/Tritium_7/set_1/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_2/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_3/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_4/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_5/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_6/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_7/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_8/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_9/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_10/p98(30s)(HV1=13000)",
    // ];

    // let files = [
    //     "/data/numass-server/2022_12/Tritium_7/set_1/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_2/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_3/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_4/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_5/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_6/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_7/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_8/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_9/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_10/p120(30s)(HV1=12000)",
    // ];

    let mut counts = [0; 7];

    let crosses = {
        let mut crosses = vec![];

        let handles = files
            .iter()
            .map(|filepath| {
                let mut counts = [0; 7];

                let filepath = PathBuf::from(filepath);
                tokio::spawn(async move {
                    let mut crosses = BTreeMap::new();

                    let point = load_point(&filepath).await;
                    
                    for channel in &point.channels {
                        for block in &channel.blocks {
                            for frame in &block.frames {
                                let entry: &mut Vec<_> = crosses.entry(frame.time).or_default();
                                entry
                                    .push((channel.id as u8, process_waveform(frame) ));
                                counts[channel.id as usize] += 1;
                            }
                        }
                    }

                    let filtered_crosses = crosses
                        .iter()
                        .filter(|(_, waveforms)| waveforms.len() > 1)
                        .filter(|(_, waveforms)| {
                            let mut has_duplicate = false;

                            for idx_1 in 0..waveforms.len() - 1 {
                                for idx_2 in idx_1 + 1..waveforms.len() {
                                    if waveforms[idx_1].0 == waveforms[idx_2].0 {
                                        has_duplicate = true;
                                        break;
                                    }
                                }
                            }

                            !has_duplicate
                        })
                        .map(|(ch_num, waveform)| (*ch_num, waveform.to_owned()))
                        .collect::<Vec<_>>();

                    (counts, filtered_crosses)
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            let (counts_local, mut crosses_local) = handle.await.unwrap();
            // println!("{counts_local:?}");
            for idx in 0..7 {
                counts[idx] += counts_local[idx];
            }
            // let cr = ;
            crosses.append(&mut crosses_local);
        }
        crosses
    };

    let double_crosses = crosses
        .iter()
        .filter(|(_, waveforms)| waveforms.len() == 2)
        .filter(|(_, waveforms)| waveforms[0].0 != waveforms[1].0)
        .map(|(ch_num, waveforms)| (*ch_num, waveforms.clone()))
        .collect::<Vec<_>>();

    let mut homogeneity = BTreeMap::new();

    for (_, waveforms) in &double_crosses {
        let ch_1 = waveforms[0].0 + 1;
        let ch_2 = waveforms[1].0 + 1;

        let idx = if ch_1 < ch_2 {
            [ch_1, ch_2]
        } else {
            [ch_2, ch_1]
        };

        let counter = homogeneity.entry(idx).or_insert(0usize);
        *counter += 1;
    }

    let mut homogeneity_sorted = homogeneity.iter().collect::<Vec<_>>();
    homogeneity_sorted.sort_by_key(|(k, _)| k[0] * 10 + k[1]);

    println!("total counts");
    println!("channel\tcounts");
    for (idx, counts) in counts.iter().enumerate() {
        println!("{}\t{counts}", idx + 1)
    }
    println!();

    println!("total crosses\t{}", crosses.len());
    println!("double crosses\t{}", double_crosses.len());

    println!("crosses");
    println!("ch_0\tch_1\tcounts");
    for ([ch_0, ch_1], counts) in homogeneity_sorted {
        println!("{ch_0}\t{ch_1}\t{counts}");
    }

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "crosses",
        native_options,
        Box::new(|_| {
            Box::new(CrossesViewer {
                crosses: double_crosses,
                filtered: vec![],
                ch_enabled: [false; 7],
                current: 0,
            })
        })
    ).unwrap();

    // println!("{crosses:?}");
}

#[cfg(not(target_arch = "wasm32"))]
type Waveform = (u8, ProcessedWaveform);

#[cfg(not(target_arch = "wasm32"))]
struct CrossesViewer {
    crosses: Vec<(u64, Vec<Waveform>)>,
    filtered: Vec<(u64, Vec<Waveform>)>,
    ch_enabled: [bool; 7],
    current: usize,
}

#[cfg(not(target_arch = "wasm32"))]
impl eframe::App for CrossesViewer {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {

        ctx.input(|i| {
            if i.key_pressed(eframe::egui::Key::ArrowRight) &&  self.current < self.filtered.len() - 1
            {
                self.current += 1;
            }
            if i.key_pressed(eframe::egui::Key::ArrowLeft) && self.current > 0 {
                self.current -= 1;
            }
        });

        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            ui.style_mut().spacing.slider_width = frame.info().window_info.size.x - 150.0;

            ui.horizontal(|ui| {
                ui.label("Enabled: ");
                for idx in 0..7 {
                    ui.checkbox(&mut self.ch_enabled[idx], format!("ch {}", idx + 1));
                }

                if ui.button("apply").clicked() {
                    self.current = 0;
                    self.filtered = self
                        .crosses
                        .iter()
                        .filter(|(_, waveforms)| {
                            let mut contains = true;
                            for (ch, _) in waveforms {
                                if !self.ch_enabled[*ch as usize] {
                                    contains = false;
                                    break;
                                }
                            }
                            contains
                        })
                        .map(|(time, waveforms)| (*time, waveforms.to_owned()))
                        .collect::<Vec<_>>();
                }
            });

            ui.horizontal(|ui| {
                ui.add(
                    eframe::egui::Slider::new(&mut self.current, 0..=self.filtered.len() - 1)
                        .step_by(1.0),
                );
                if ui.button("<").clicked() && self.current > 0 {
                    self.current -= 1;
                }
                if ui.button(">").clicked() && self.current < self.filtered.len() - 1 {
                    self.current += 1;
                }
            });

            eframe::egui::plot::Plot::new("waveforms")
                .legend(eframe::egui::plot::Legend {
                    text_style: eframe::egui::TextStyle::Body,
                    background_alpha: 1.0,
                    position: eframe::egui::plot::Corner::RightTop,
                })
                .x_axis_formatter(|value, _| format!("{:.3} μs", (value * 8.0) / 1000.0))
                .show(ui, |plot_ui| {
                    if !self.filtered.is_empty() {
                        let (_, waveform) = &self.filtered[self.current];
                        for (ch_num, waveform) in waveform {
                            waveform.clone().draw_egui(
                                plot_ui,
                                Some(&format!("ch #{}", ch_num + 1)), 
                                Some(color_for_index((*ch_num) as usize)),
                                None, None
                            );
                        }
                    }
                });
        });
    }
}

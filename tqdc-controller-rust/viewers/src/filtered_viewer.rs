use crate::{app::color_same_as_egui, backend::DeviceFrame};

pub struct FilteredViewer {
    pub independent: Vec<(DeviceFrame, Vec<DeviceFrame>)>,
    pub current: usize,
}

impl eframe::App for FilteredViewer {
    #[allow(unused_variables)]
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        ctx.input(|i| {
            if i.key_pressed(eframe::egui::Key::ArrowRight)
                && self.current < self.independent.len() - 1
            {
                self.current += 1;
            }
            if i.key_pressed(eframe::egui::Key::ArrowLeft) && self.current > 0 {
                self.current -= 1;
            }
        });

        let (current, neighbors) = &self.independent[self.current];

        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            #[cfg(not(target_arch = "wasm32"))]
            let width = frame.info().window_info.size.x;
            #[cfg(target_arch = "wasm32")]
            let width = eframe::web_sys::window()
                .unwrap()
                .inner_width()
                .unwrap()
                .as_f64()
                .unwrap() as f32;

            ui.style_mut().spacing.slider_width = width - 200.0;

            ui.horizontal(|ui| {
                ui.add(
                    eframe::egui::Slider::new(&mut self.current, 0..=self.independent.len() - 1)
                        .step_by(1.0),
                );
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
                    background_alpha: 1.0,
                    position: eframe::egui::plot::Corner::RightTop,
                })
                .x_axis_formatter(|value, _| format!("{:.3} μs", (value * 8.0) / 1000.0))
                .show(ui, |plot_ui| {
                    for (ch_id, waveform) in &current.waveforms {
                        plot_ui.line(
                            eframe::egui::plot::Line::new(
                                waveform
                                    .iter()
                                    .enumerate()
                                    .map(|(x, y)| [x as f64, *y as f64])
                                    .collect::<Vec<_>>(),
                            )
                            .width(3.0)
                            .color(color_same_as_egui(*ch_id as usize))
                            .name(format!("ch# {}", ch_id + 1)),
                        );
                    }

                    neighbors
                        .iter()
                        .for_each(|DeviceFrame { time, waveforms }| {
                            let delta = (*time as f64 - current.time as f64) / 8.0;

                            for (ch_id, waveform) in waveforms {
                                plot_ui.line(
                                    eframe::egui::plot::Line::new(
                                        waveform
                                            .iter()
                                            .enumerate()
                                            .map(|(x, y)| [x as f64 + delta, *y as f64])
                                            .collect::<Vec<_>>(),
                                    )
                                    .width(1.0)
                                    .color(color_same_as_egui(*ch_id as usize))
                                    .name(format!("ch# {}", ch_id + 1)),
                                );
                            }
                        });
                });
        });
    }
}

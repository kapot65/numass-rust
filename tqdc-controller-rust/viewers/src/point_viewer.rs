use crate::app::color_same_as_egui;

pub struct PointViewer {
    pub chunks: Vec<Vec<(u8, Vec<[f64; 2]>)>>,
    pub current_chunk: usize,
}

impl eframe::App for PointViewer {
    #[allow(unused_variables)]
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.input(|i| {
            if i.key_pressed(eframe::egui::Key::ArrowRight)
                && self.current_chunk < self.chunks.len() - 1
            {
                self.current_chunk += 1;
            }
            if i.key_pressed(eframe::egui::Key::ArrowLeft) && self.current_chunk > 0 {
                self.current_chunk -= 1;
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            #[cfg(not(target_arch = "wasm32"))]
            let width = frame.info().window_info.size.x;
            #[cfg(target_arch = "wasm32")]
            let width = eframe::web_sys::window()
                .unwrap()
                .inner_width()
                .unwrap()
                .as_f64()
                .unwrap() as f32;

            ui.style_mut().spacing.slider_width = width - 150.0;

            ui.horizontal(|ui| {
                ui.add(
                    egui::Slider::new(&mut self.current_chunk, 0..=self.chunks.len() - 1)
                        .suffix(" ms")
                        .step_by(1.0),
                );
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
                    background_alpha: 1.0,
                    position: egui::plot::Corner::RightTop,
                })
                .x_axis_formatter(|value, _| format!("{value:.3} μs"))
                .show(ui, |plot_ui| {
                    for (ch_num, x) in self.chunks[self.current_chunk].clone() {
                        plot_ui.line(
                            egui::plot::Line::new(x)
                                .color(color_same_as_egui((ch_num) as usize))
                                .name(format!("ch #{}", ch_num + 1)),
                        );
                    }
                });
        });
    }
}

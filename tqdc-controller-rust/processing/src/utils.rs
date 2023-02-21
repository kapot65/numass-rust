#[cfg(not(target_arch = "wasm32"))]
use eframe::epaint::{color::Hsva, Color32};

#[cfg(not(target_arch = "wasm32"))]
// copy-pasted from egui::widgets::plot::PlotUi::auto_color
pub fn color_same_as_egui(idx: usize) -> Color32 {
    let golden_ratio = (5.0_f32.sqrt() - 1.0) / 2.0; // 0.61803398875
    let h = idx as f32 * golden_ratio;
    Hsva::new(h, 0.85, 0.5, 1.0).into() // TODO(emilk): OkLab or some other perspective color space
}

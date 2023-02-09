use eframe::epaint::{Color32, color::Hsva};

// copy-pasted from egui::widgets::plot::PlotUi::auto_color
pub fn color_same_as_egui(idx: usize) -> Color32 {
    let golden_ratio = (5.0_f32.sqrt() - 1.0) / 2.0; // 0.61803398875
    let h = idx as f32 * golden_ratio;
    Hsva::new(h, 0.85, 0.5, 1.0).into() // TODO(emilk): OkLab or some other perspective color space
}

pub fn channel_colors() -> [Color32; 7]{
    [
        color_same_as_egui(0), 
        color_same_as_egui(1), 
        color_same_as_egui(2), 
        color_same_as_egui(3), 
        color_same_as_egui(4),
        color_same_as_egui(5),
        color_same_as_egui(6)
    ]
}
use std::{collections::BTreeMap, ops::Range};

use serde::{Deserialize, Serialize};

#[cfg(feature = "egui")]
use {
    egui_plot::{Line, PlotUi},
    egui::Color32,
    crate::utils::color_for_index
};

#[cfg(feature = "plotly")]
use plotly::{common::{Line as PlotlyLine, Mode, LineShape}, Scatter, Plot};

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct HistogramParams {
    pub range: Range<f32>,
    pub bins: usize
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointHistogram {
    pub x: Vec<f32>,
    pub channels: BTreeMap<u8, Vec<f32>>,
    pub step: f32,
    bins: usize,
    range: Range<f32>,
}

impl From<HistogramParams> for PointHistogram {
    fn from(params: HistogramParams) -> Self {
        Self::new(params.range, params.bins)
    }
}

// TODO: change to generic constant channel histogram
impl PointHistogram {
    pub fn new(range: Range<f32>, bins: usize) -> Self {
        let step = (range.end - range.start) / bins as f32;
        PointHistogram {
            x: (0..bins)
                .map(|idx| range.start + step * (idx as f32) + step / 2.0)
                .collect::<Vec<f32>>(),
            step,
            range,
            bins,
            channels: BTreeMap::new(),
        }
    }

    pub fn new_step(range: Range<f32>, step: f32) -> Self {
        let bins = ((range.end - range.start).abs() / step) as usize;
        PointHistogram {
            x: (0..bins)
                .map(|idx| range.start + step * (idx as f32) + step / 2.0)
                .collect::<Vec<f32>>(),
            step,
            range,
            bins,
            channels: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, ch_num: u8, amplitude: f32) {

        let min = self.range.start;
        let max = self.range.end;

        if amplitude > min && amplitude < max {
            let y = self
                .channels
                .entry(ch_num)
                .or_insert_with(|| vec![0.0; self.bins]);
            let bin = ((amplitude - min) / self.step) as usize;
            y[bin] += 1.0;
        }
    }

    pub fn add_batch(&mut self, ch_num: u8, amplitudes: Vec<f32>) {
        let min = self.range.start;
        let y = self
            .channels
            .entry(ch_num)
            .or_insert_with(|| vec![0.0; self.bins]);

        for amplitude in amplitudes {
            let idx = (amplitude - min) / self.step;
            if idx >= 0.0 && idx < self.bins as f32 {
                y[idx as usize] += 1.0;
            }
        }
    }

    pub fn events(&self, window: Option<Range<f32>>) -> BTreeMap<u8, usize> {

        let (left_border, right_border) = if let Some(window) = window {
            (window.start, window.end)
        } else {
            (self.range.start, self.range.end)
        };

        self.channels.iter().map(|(ch_num, channel)| {
            let mut events_in_window = 0;
            channel.iter().enumerate().for_each(|(idx, y)| {
                if self.x[idx] > left_border && self.x[idx] < right_border {
                    events_in_window += *y as usize;
                }
            });
            (*ch_num, events_in_window)
        }).collect::<BTreeMap<_, _>>()
    }

    pub fn events_all(&self, window: Option<Range<f32>>) -> usize {
        self.events(window).values().sum()
    }

    pub fn to_csv(&self, separator: char) -> String {
        let mut data = String::new();
        {
            let mut row = String::new();
            row.push_str(&format!("bin{separator}"));
            for ch_num in self.channels.keys() {
                row.push_str(&format!("ch {}{separator}", *ch_num + 1));
            }
            row.push('\n');

            data.push_str(&row);
        }

        for (idx, bin) in self.x.iter().enumerate() {
            let mut row = String::new();

            row.push_str(&format!("{bin:.4}{separator}"));
            for val in self.channels.values() {
                row.push_str(&format!("{}{separator}", val[idx]));
            }
            row.push('\n');
            data.push_str(&row);
        }

        data
    }

    #[cfg(feature = "egui")]
    fn build_egui_hist(&self, y: &[f32])  -> Vec<[f64; 2]> {
        y.iter().enumerate().flat_map(|(idx, y)| {
            [
                [(self.x[idx] - self.step / 2.0)  as f64, *y as f64],
                [(self.x[idx] + self.step / 2.0)  as f64, *y as f64]
            ]
        }).collect::<Vec<_>>()
    }

    pub fn merge_channels(&self) -> Vec<f32> {
        let mut y_all: Vec<f32> = vec![0.0; self.x.len()];
        self.channels.iter().for_each(|(_, y)| {
            for (idx, val) in y.iter().enumerate() {
                y_all[idx] += val;
            }
        });
        y_all
    }

    #[cfg(feature = "egui")]
    pub fn draw_egui(&self, plot_ui: &mut PlotUi, name: Option<&str>, thickness: Option<f32>, color: Option<Color32>) {
        let bounds = plot_ui.plot_bounds();
        let left_border = bounds.min()[0] as f32;
        let right_border = bounds.max()[0] as f32;

        let events_in_window = self.events_all(Some(left_border..right_border));
        let mut line = Line::new(self.build_egui_hist(&self.merge_channels()));
        if let Some(color) = color {
            line = line.color(color);
        }
        if let Some(thickness) = thickness {
            line = line.width(thickness);
        }
        if let Some(name) = name {
            line = line.name(format!("{name} ({events_in_window})"));
        }
        plot_ui.line(line)
    }

    #[cfg(feature = "egui")]
    pub fn draw_egui_each_channel(&self, plot_ui: &mut PlotUi, thickness: Option<f32>) {

        let bounds = plot_ui.plot_bounds();
        let left_border = bounds.min()[0] as f32;
        let right_border = bounds.max()[0] as f32;

        let events_in_window = self.events(Some(left_border..right_border));
        self.channels.iter().for_each(|(ch_num, channel)| {

            let events_in_window = events_in_window.get(&ch_num).unwrap_or(&0);

            let mut line = Line::new(self.build_egui_hist(channel))
                .color(color_for_index(*ch_num as usize))
                .name(format!("ch #{}\t({events_in_window})", ch_num + 1));
            if let Some(thickness) = thickness {
                line = line.width(thickness);
            }

            plot_ui.line(line)
        });
    }

    #[cfg(feature = "plotly")]
    pub fn draw_plotly(&self, plot: &mut Plot, name: Option<&str>) {
        let mut line = Scatter::new(self.x.clone(), self.merge_channels())
            .mode(Mode::Lines).line(PlotlyLine::new().shape(LineShape::Hvh));

        if let Some(name) = name {
            line = line.name(name);
        }

        plot.add_trace(line);
    }

    #[cfg(feature = "plotly")]
    pub fn draw_plotly_each_channel(&self, plot: &mut Plot) {
        use crate::utils::color_for_index_str;

        self.channels.iter().for_each(|(ch_num, channel)| {
            let mut line = Scatter::new(self.x.clone(), channel.clone())
                .mode(Mode::Lines).line(
                    PlotlyLine::new()
                    .color(color_for_index_str(*ch_num as usize))
                    .shape(LineShape::Hvh));
                
            line = line.name(format!("ch #{}", ch_num + 1));

            plot.add_trace(line);
        });
    }
}

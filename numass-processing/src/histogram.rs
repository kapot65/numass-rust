use std::{collections::BTreeMap, ops::Range};

use serde::{Deserialize, Serialize};

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

    pub fn add(&mut self, ch_num: u8, amplitude: f32) {
        let amplitude = amplitude;

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
}
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointHistogram {
    pub x: Vec<f32>,
    pub channels: BTreeMap<u8, Vec<f32>>,
    pub step: f32,
    bins: usize,
    range: (f32, f32),
}

impl PointHistogram {
    pub fn new(range: (f32, f32), bins: usize) -> Self {
        let (min, max) = range;
        let step = (max - min) / bins as f32;
        PointHistogram {
            x: (0..bins)
                .map(|idx| min + step * (idx as f32) + step / 2.0)
                .collect::<Vec<f32>>(),
            step,
            range,
            bins,
            channels: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, ch_num: u8, amplitude: f32) {
        let amplitude = amplitude;
        let (min, max) = self.range;
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
        let (min, _) = self.range;
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

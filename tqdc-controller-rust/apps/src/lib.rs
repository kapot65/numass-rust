pub mod defaults;

use dataforge::{protos::rsb_event::{self, Point}, ZeroSuppressionParams};
use tokio::io::AsyncWriteExt;
use std::collections::HashMap;
use tqdc::mlink::MStreamFragment;

#[derive(Debug, Clone)]
pub struct PointHistogramm {
    pub x: Vec<f32>,
    pub channels: HashMap<u8, Vec<f32>>,
    pub step: f32,
    bins: usize,
    range: (i32, i32)
}

impl PointHistogramm {
    pub fn new(range: (i32, i32), bins: usize) -> Self {
        let (min, max) = range;
        let step = (max - min) as f32 / bins as f32;
        PointHistogramm {
            x: (0..bins).map(|idx| {
                min as f32 + step * (idx as f32) + step / 2.0
            }).collect::<Vec<f32>>(),
            step,
            range,
            bins,
            channels: HashMap::new()
        }
    }

    pub fn add(&mut self, ch_num: u8, amplitude: i16) {
        let amplitude = amplitude as i32;
        let (min, max) = self.range;
        if amplitude > min && amplitude < max {
            let y = self.channels.entry(ch_num).or_insert_with(|| vec![0.0; self.bins]);
            let bin = ((amplitude - min) as f32 / self.step) as usize;
            y[bin] += 1.0;
        }
    }

    pub fn add_batch(&mut self, ch_num: u8, amplitudes: Vec<i16>) {
        let (min, _) = self.range;
        let y = self.channels.entry(ch_num).or_insert_with(|| vec![0.0; self.bins]);
        for amplitude in amplitudes {
            let idx = (amplitude as i32 - min) as f32 / self.step;
            if idx >= 0.0 && idx < self.bins as f32 {
                y[idx as usize] += 1.0;
            }
        };
    }
}

pub async fn point_to_histogramm(point: &Point, range: (i32, i32), bins: usize) -> PointHistogramm {

    let mut histogram = PointHistogramm::new(range, bins);

    for channel in &point.channels {

        let amplitudes = channel.blocks.iter().flat_map(|block| {
            block.frames.iter().map(|frame| {
                let waveform_len = frame.data.len() / 2;

                let waveform = (0..waveform_len).map(|idx| {
                    i16::from_le_bytes(frame.data[idx*2..idx*2+2].try_into().unwrap())
                });

                let first = waveform.clone().take(4).sum::<i16>() / 4;

                let waveform_normed = waveform.map(|ampl| {
                    ampl - first
                });

                waveform_normed.max().unwrap()
            })
        }).collect::<Vec<_>>();

        histogram.add_batch(channel.id as u8, amplitudes);
    };

    histogram
}

pub async fn events_to_point(events: Vec<MStreamFragment>, zero_suppression: Option<ZeroSuppressionParams>) -> tokio::io::Result<rsb_event::Point> {

    let mut frames_per_channel: HashMap<u8, Vec<rsb_event::point::channel::block::Frame>> = HashMap::new();
    let mut begin_time: Option<u128> = None;

    for frame in events {
        for channel in frame.channels {

            let append = match zero_suppression {
                Some(params) => {
                    let baseline = channel.waveform.iter().take(params.head_size).sum::<i16>() as f32 / params.head_size as f32;
                    let max = *channel.waveform.iter().max().unwrap() as f32 - baseline;
                    max > params.threshold as f32
                }
                None => true
            };

            if append {
                let tai_nsec = (frame.tai_sec as u128) * (1e9 as u128) + ((frame.tai_nano_sec as u128) / 4u128);
                let begin = begin_time.get_or_insert(
                    tai_nsec - 1_000_000); // TODO: make offset to metadata

                let mut frame = rsb_event::point::channel::block::Frame::new();
                frame.time = (tai_nsec - *begin) as u64;

                frame.data = Vec::with_capacity(channel.waveform.capacity() * 2);
                for bin in channel.waveform {
                    frame.data.write_i16_le(bin / 4).await?;
                }

                frames_per_channel.entry(channel.channel_number).or_default().push(
                    frame
                );
            } 
        }
    }

    let point = {
        let mut point = rsb_event::Point::new();
        
        for (id, frames) in frames_per_channel {
            let mut channel = rsb_event::point::Channel::new();
            channel.id = id as u64;

            let mut block = rsb_event::point::channel::Block::new();

            block.time = begin_time.unwrap() as u64;
            // block.length = (acquisition_time_ms as u64) * 1_000_000;
            block.bin_size = 8;
            block.frames.extend(frames);

            channel.blocks.push(block);

            point.channels.push(channel);
        } 
        point
    };

    Ok(point)
}
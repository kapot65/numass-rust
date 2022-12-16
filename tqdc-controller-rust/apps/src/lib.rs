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
    range: (f32, f32)
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Algorithm {
    Max,
    Likhovid {
        left: usize,
        right: usize
    }
}

 // TODO remove hardcode
const KEV_COEFF_MAX: [[f32; 2]; 7] = [
    [0.0597, 0.1481],
    [0.0608, 0.1942],
    [0.0634, 0.0839],
    [0.0627, 0.1587],
    [0.0626, 0.1675],
    [0.0675, 0.0930],
    [0.0580, 0.0923],
];

const KEV_COEFF_LIKHOVID: [[f32; 2]; 7] = [
    [0.134678, 0.09647 ],
    [0.141536, 0.060275],
    [0.147718, 0.027412],
    [0.150288, 0.038774],
    [0.15131 , 0.071923],
    [0.15336 , 0.029206],
    [0.136762, 0.041848]
];

#[derive(PartialEq, Clone, Copy, Debug)]
pub struct ProcessingParams {
    pub algorithm: Algorithm,
    pub convert_to_kev: bool,
    // TODO: add to KeV corrections
    // TODO: refactor to separate struct and merge with PointHistogram
    pub hist_min: f32,
    pub hist_max: f32,
    pub hist_bins: usize
}

impl PointHistogramm {
    pub fn new(range: (f32, f32), bins: usize) -> Self {
        let (min, max) = range;
        let step = (max - min) / bins as f32;
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

    pub fn add(&mut self, ch_num: u8, amplitude: f32) {
        let amplitude = amplitude;
        let (min, max) = self.range;
        if amplitude > min && amplitude < max {
            let y = self.channels.entry(ch_num).or_insert_with(|| vec![0.0; self.bins]);
            let bin = ((amplitude - min) as f32 / self.step) as usize;
            y[bin] += 1.0;
        }
    }

    pub fn add_batch(&mut self, ch_num: u8, amplitudes: Vec<f32>) {
        let (min, _) = self.range;
        let y = self.channels.entry(ch_num).or_insert_with(|| vec![0.0; self.bins]);


        for amplitude in amplitudes {
            let idx = (amplitude - min) / self.step;
            if idx >= 0.0 && idx < self.bins as f32 {
                y[idx as usize] += 1.0;
            }
        };
    }
}

pub async fn point_to_histogramm(point: &Point, params: ProcessingParams) -> PointHistogramm {

    let range = (params.hist_min, params.hist_max);
    let bins = params.hist_bins;

    match params.algorithm {
        
        Algorithm::Max => {

            let mut histogram = PointHistogramm::new(range, bins);

            for channel in &point.channels {

                let amplitudes = channel.blocks.iter().flat_map(|block| {
                    block.frames.iter().map(|frame| {
                        let waveform_len = frame.data.len() / 2;
        
                        let waveform = (0..waveform_len).map(|idx| {
                            i16::from_le_bytes(frame.data[idx*2..idx*2+2].try_into().unwrap())
                        });
        
                        let first = waveform.clone().take(16).sum::<i16>() / 16;
        
                        let waveform_normed = waveform.map(|ampl| {
                            ampl - first
                        });
        
                        let x = waveform_normed.max().unwrap() as f32;

                        if params.convert_to_kev {
                            let [a, b] = KEV_COEFF_MAX[channel.id as usize];
                            a * x + b
                        } else {
                            x
                        }
                        
                    })
                }).collect::<Vec<_>>();
        
                histogram.add_batch(channel.id as u8, amplitudes);
            };
        
            histogram
        }

        Algorithm::Likhovid { left, right } => {

            let mut histogram = PointHistogramm::new(range, bins);

            for channel in &point.channels {

                let amplitudes = channel.blocks.iter().flat_map(|block| {
                    block.frames.iter().map(|frame| {
                        let waveform_len = frame.data.len() / 2;

                        let waveform = (0..waveform_len).map(|idx| {
                            i16::from_le_bytes(frame.data[idx*2..idx*2+2].try_into().unwrap())
                        });

                        let waveform = waveform.collect::<Vec<_>>();

                        let baseline = waveform.iter().take(16).sum::<i16>() as f32 / 16.0;
                        let (argmax, _) = waveform.iter().enumerate().max_by_key(|(_, amp)| *amp).unwrap();

                        let left = if argmax >= left {argmax - left} else { 0 };
                        let right = std::cmp::min(waveform_len, argmax + right);
                        let crop =  &waveform[left..right];

                        let amplitude = crop.iter().sum::<i16>() as f32 / crop.len() as f32;


                        let x = amplitude - baseline;

                        if params.convert_to_kev {
                            let [a, b] = KEV_COEFF_LIKHOVID[channel.id as usize];
                            a * x + b
                        } else {
                            x
                        }
                    })
                }).collect::<Vec<_>>();

                histogram.add_batch(channel.id as u8, amplitudes);
            };

            histogram

        }
    }    
}

pub async fn events_to_point(events: Vec<MStreamFragment>, zero_suppression: Option<ZeroSuppressionParams>) -> tokio::io::Result<rsb_event::Point> {

    let mut frames_per_channel: HashMap<u8, Vec<rsb_event::point::channel::block::Frame>> = HashMap::new();
    let mut begin_time: Option<u128> = None;

    for frame in events {
        for channel in frame.channels {

            let append = match zero_suppression {
                Some(params) => {
                    let baseline = channel.waveform.iter().take(params.baseline).sum::<i16>() / params.baseline as i16;
                    let max = *channel.waveform.iter().max().unwrap() - baseline;
                    max > params.threshold * 4
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
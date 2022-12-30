pub mod defaults;

use std::collections::HashMap;

use processing::{frame_to_event, Algorithm, convert_to_kev};
use tokio::io::AsyncWriteExt;

use tqdc::mlink::MStreamFragment;
use dataforge::{protos::rsb_event::{self, Point}, ZeroSuppressionParams};

#[derive(Debug, Clone)]
pub struct PointHistogramm {
    pub x: Vec<f32>,
    pub channels: HashMap<u8, Vec<f32>>,
    pub step: f32,
    bins: usize,
    range: (f32, f32)
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub struct ProcessingParams {
    pub algorithm: Algorithm,
    pub convert_to_kev: bool,
    pub merge_close_events: bool,
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
                min + step * (idx as f32) + step / 2.0
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
            let bin = ((amplitude - min) / self.step) as usize;
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

    let mut histogram = PointHistogramm::new(range, bins);

    let mut events_per_channel = point.channels.iter().map(|channel| {
        let amplitudes = channel.blocks.iter().flat_map(|block| {
            block.frames.iter().map(|frame| {
                let (_, amp) = frame_to_event(frame, &params.algorithm);
                Some((
                    frame.time,
                    if params.convert_to_kev {
                        convert_to_kev(&amp, channel.id as u8, &params.algorithm)
                    } else {
                        amp
                    }
                ))
            })
        }).collect::<Vec<_>>();
        (channel.id as u8, amplitudes)
    }).collect::<Vec<_>>();

    if params.merge_close_events {

        for (_, channel) in &mut events_per_channel {
            channel.sort_by_key(|k| k.unwrap().0);
        }
        
        for ch_id in [5usize, 0, 1, 2, 3, 4, 6] {

            if ch_id >= events_per_channel.len() {
                continue;
            }

            let mut start_idxs = vec![0usize; 7];
            for ev_id in 0..events_per_channel[ch_id].1.len() {
    
                if let Some((time1, ampl1)) = events_per_channel[ch_id].1[ev_id] {
                    for ch_id_2 in 0..events_per_channel.len() {
    
                        if ch_id == ch_id_2 {
                            continue;
                        }
    
                        for ev_id_2 in start_idxs[ch_id_2]..events_per_channel[ch_id_2].1.len() {
                            if let Some((time2, ampl2)) = events_per_channel[ch_id_2].1[ev_id_2] {
                                match time2.cmp(&time1) {
                                    std::cmp::Ordering::Less => {}
                                    std::cmp::Ordering::Equal => {
                                        events_per_channel[ch_id].1[ev_id] = Some((time1, ampl1 + ampl2));
                                        events_per_channel[ch_id_2].1[ev_id_2] = None;
                                    }
                                    std::cmp::Ordering::Greater => {
                                        if ev_id_2 != 0 {
                                            start_idxs[ch_id_2] = ev_id_2 - 1;
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    
                }
            }
        }
    }
    
    for (ch_num, events) in events_per_channel {
        let amps = events.iter().filter_map(|val| val.map(|(_, amp)| amp)).collect::<Vec<_>>();
        histogram.add_batch(ch_num, amps);
    }

    histogram
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
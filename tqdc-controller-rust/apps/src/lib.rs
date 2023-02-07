pub mod defaults;

use std::collections::{HashMap, BTreeMap};


use tokio::io::AsyncWriteExt;

use tqdc::mlink::MStreamFragment;
use processing::{waveform_to_event, Algorithm, convert_to_kev, frame_to_waveform, histogram::PointHistogram};
use numass::{protos::rsb_event::{self, Point}, ZeroSuppressionParams};

#[derive(PartialEq, Clone, Copy, Debug)]
pub struct ProcessingParams {
    pub algorithm: Algorithm,
    pub convert_to_kev: bool,
    pub merge_close_events: bool,
    pub merge_map: [[bool; 7]; 7],
    // TODO: add to KeV corrections
    // TODO: refactor to separate struct and merge with PointHistogram
    pub hist_min: f32,
    pub hist_max: f32,
    pub hist_bins: usize
}

pub async fn point_to_histogramm(point: &Point, params: ProcessingParams) -> PointHistogram {

    let range = (params.hist_min, params.hist_max);
    let bins = params.hist_bins;

    let mut histogram = PointHistogram::new(range, bins);

    let mut frames = BTreeMap::new();

    for channel in &point.channels {
        for block in &channel.blocks {
            for frame in &block.frames {
                let entry = frames.entry(frame.time).or_insert(BTreeMap::new());

                let waveform = frame_to_waveform(frame);
                let amp = waveform_to_event(&waveform, &params.algorithm).1;

                let amp = if params.convert_to_kev {
                    convert_to_kev(&amp, channel.id as u8, &params.algorithm)
                } else { amp };

                entry.insert(channel.id as usize, amp);
            }
        }
    }

    for (_, mut channels) in frames {
        if params.merge_close_events {
            for ch_1 in 0..7 {
                for ch_2 in 0..7 {
                    if params.merge_map[ch_1][ch_2] && channels.contains_key(&ch_1) && channels.contains_key(&ch_2) {
                        let amp2  = channels.get(&ch_2).unwrap().to_owned();
                        channels.entry(ch_1).and_modify(|amp| *amp += amp2);
                        channels.remove_entry(&ch_2).unwrap();
                    }
                }
            }
        }

        for (ch_num, amplitude) in channels {
            histogram.add(ch_num as u8, amplitude)
        }
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
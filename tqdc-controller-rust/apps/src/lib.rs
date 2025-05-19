pub mod defaults;

use std::collections::HashMap;

use tokio::io::AsyncWriteExt;

use processing::numass::{protos::rsb_event, ZeroSuppressionParams};
use tqdc::mstream::MStreamADCBlocks;

pub async fn events_to_point(
    events: Vec<MStreamADCBlocks>,
    zero_suppression: Option<ZeroSuppressionParams>,
) -> tokio::io::Result<rsb_event::Point> {
    let mut frames_per_channel: HashMap<u8, Vec<rsb_event::point::channel::block::Frame>> =
        HashMap::new();
    let mut begin_time: Option<u128> = None;

    for frame in events {
        for channel in frame.adc_blocks {
            let append = match &zero_suppression {
                Some(params) => {

                    let mut crop = None;
                    for (idx, window) in channel.waveform.windows(30).enumerate() {
                        if window[0].abs_diff(window[29]) > 5000 {
                            crop = Some(idx);
                            break;
                        }
                    }

                    // TODO: check this on real data
                    let waveform = channel.waveform.iter().map(|&x| x as f32).collect::<Vec<_>>();
                    let waveform = if let Some(fir) = &params.fir {
                        waveform.windows(fir.len()).map(|window| {
                            window.iter().zip(fir).map(|(&amp, &coeff)| amp * coeff).sum::<f32>()
                        }).collect::<Vec<_>>()
                    } else {
                        waveform
                    };

                    let threshold = if channel.ch_num == 0 && params.th_1.is_some() {
                        params.th_1.unwrap()
                    } else if channel.ch_num == 1 && params.th_2.is_some() {
                        params.th_2.unwrap()
                    } else if channel.ch_num == 2 && params.th_3.is_some() {
                        params.th_3.unwrap()
                    } else if channel.ch_num == 3 && params.th_4.is_some() {
                        params.th_4.unwrap()
                    } else if channel.ch_num == 4 && params.th_5.is_some() {
                        params.th_5.unwrap()
                    } else if channel.ch_num == 5 && params.th_6.is_some() {
                        params.th_6.unwrap()
                    } else if channel.ch_num == 6 && params.th_7.is_some() {
                        params.th_7.unwrap()
                    } else {
                        params.threshold
                    };

                    let baseline = if let Some(crop) = crop {
                        if crop < params.baseline {
                            waveform.iter().skip(crop + 120).take(params.baseline).sum::<f32>()/ params.baseline as f32
                        } else {
                            waveform.iter().take(params.baseline).sum::<f32>()/ params.baseline as f32
                        }
                    } else { // not enough bins on frame begin - process after reset
                        waveform.iter().take(params.baseline).sum::<f32>()/ params.baseline as f32
                    };
                    // convert into i16 in order to be able to find max
                    let max = waveform.into_iter()
                        .map(|val| val as i16);

                    let max = if let Some(crop) = crop {
                        if crop == 0 { // not enough bins on frame begin - process after reset
                            max.skip(120).max().unwrap() as f32 - baseline
                        } else {
                            max.take(crop).max().unwrap() as f32 - baseline
                        }              
                    } else {
                        max.max().unwrap() as f32 - baseline
                    };
                    max > (threshold * 4) as f32
                }
                None => true,
            };

            if append {
                let tai_nsec = (frame.tai_sec as u128) * (1e9 as u128)
                    + ((frame.tai_nano_sec as u128) / 4u128);
                let begin = begin_time.get_or_insert(tai_nsec - 1_000_000); // TODO: make offset to metadata

                let mut frame = rsb_event::point::channel::block::Frame::new();
                frame.time = (tai_nsec - *begin) as u64;

                frame.data = Vec::with_capacity(channel.waveform.capacity() * 2);
                for bin in channel.waveform {
                    frame.data.write_i16_le(bin / 4).await?;
                }

                frames_per_channel
                    .entry(channel.ch_num)
                    .or_default()
                    .push(frame);
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

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
            let append = match zero_suppression {
                Some(params) => {
                    let baseline = channel.waveform.iter().take(params.baseline).sum::<i16>()
                        / params.baseline as i16;
                    let max = *channel.waveform.iter().max().unwrap() - baseline;
                    max > params.threshold * 4
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

use dataforge::protos::rsb_event;
use tokio::io::AsyncWriteExt;
use std::collections::HashMap;
use tqdc::mlink::MLinkEventHeader;

pub async fn events_to_point(events: Vec<MLinkEventHeader>) -> tokio::io::Result<rsb_event::Point> {

    // HashMap<u16, Vec<rsb_event::point::channel::block::Frame>>
    let mut frames_per_channel = HashMap::new();
    let mut begin_time: Option<u128> = None;

    for frame in events {
        for channel in frame.channels {
            let tai_nsec = (frame.tai_sec as u128) * (1e9 as u128) + ((frame.tai_nano_sec as u128) / 4u128);
            let begin = begin_time.get_or_insert(
                tai_nsec - 1000_000); // TODO: make offset to metadata

            let mut frame = rsb_event::point::channel::block::Frame::new();
            frame.time = (tai_nsec - *begin) as u64;

            frame.data = Vec::with_capacity(channel.bins.capacity());
            for bin in channel.bins {
                frame.data.write_i16_le(bin).await?;
            }

            frames_per_channel.entry(channel.id).or_insert(vec![]).push(
                frame
            );
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
use std::collections::BTreeMap;

use analysis::workspace::get_workspace;
use protobuf::Message;

use dataforge::write_df_message;
use processing::{
    numass::protos::rsb_event, storage::{load_meta, load_point}
};

#[tokio::main]
async fn main() {

    // get files in folder that matches pattern
    let files = glob::glob("/data/numass-server/2023_03/Tritium_1/set_1/p*")
        .unwrap()
        .filter_map(|res| res.ok())
        .collect::<Vec<_>>();
    // println!("{files:?}");

    // let file = PathBuf::from("/data/numass-server/2023_/Tritium_7/set_1/p52(30s)(HV1=15000)");
    let handles = files.iter().map(|file| {
        let file = file.clone();
        tokio::spawn(async move {

            let point = load_point(&file).await;
            let meta = load_meta(&file).await.unwrap();

            let mut frames = BTreeMap::new();
            
            point.channels.iter().for_each(|channel| {
                channel.blocks.iter().for_each(|block| {
                    block.frames.iter().for_each(|frame| {
                        frames.entry(frame.time).or_insert(vec![]).push((channel.id, frame.to_owned()));
                    })
                })
            });

            let mut frames_per_channel = [
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
            ];

            frames.iter().for_each(|(_, frames)| {
                if frames.len() > 1 {
                    frames.iter().for_each(|(ch_id, frame)| {
                        frames_per_channel[*ch_id as usize].push(frame.clone());
                    });
                }
            });

            let mut out_point = rsb_event::Point::new();
            frames_per_channel.iter().enumerate().for_each(|(ch_id, frames)| {
                let mut channel = rsb_event::point::Channel::new();
                channel.id = ch_id as u64;

                let mut block = rsb_event::point::channel::Block::new();

                block.time = point.channels[0].blocks[0].time;
                // block.length = (acquisition_time_ms as u64) * 1_000_000;
                block.bin_size = 8;
                block.frames.extend(frames.to_owned());

                channel.blocks.push(block);

                out_point.channels.push(channel);
            });


            let out_dir = get_workspace();
            
            let mut out_file = tokio::fs::File::create(
                out_dir.join(file.file_name().unwrap())).await.unwrap();
            
            write_df_message(
                &mut out_file, 
                meta, 
                Some(out_point.write_to_bytes().unwrap())
            ).await.unwrap();
        })
    }).collect::<Vec<_>>();


    for handle in handles {
        handle.await.unwrap();
    }
}

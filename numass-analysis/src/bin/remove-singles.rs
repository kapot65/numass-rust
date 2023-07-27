use std::path::PathBuf;

use dataforge::write_df_message;

#[tokio::main]
async fn main() {
    use protobuf::Message;
    use std::collections::BTreeMap;

    use dataforge::read_df_message;
    use processing::numass::{protos::rsb_event, NumassMeta};

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
            let mut point_file = tokio::fs::File::open(&file).await.unwrap();
            let message = read_df_message::<NumassMeta>(&mut point_file)
                .await
                .unwrap();

            let mut frames = BTreeMap::new();

            let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

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


            let out_dir: PathBuf = PathBuf::from("/home/chernov/doubles_only");
            
            let mut out_file = tokio::fs::File::create(
                out_dir.join(file.file_name().unwrap())).await.unwrap();
            
            write_df_message(
                &mut out_file, 
                message.meta, 
                Some(out_point.write_to_bytes().unwrap())
            ).await.unwrap();
        })
    }).collect::<Vec<_>>();


    for handle in handles {
        handle.await.unwrap();
    }
}

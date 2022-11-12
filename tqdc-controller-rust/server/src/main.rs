use dataforge::{extract_df_message, DFMeta, push_df_message, protos::rsb_event};
use chrono::Utc;
use tokio::task::JoinHandle;
use tokio::{io::AsyncWriteExt, sync::Mutex};
use tokio::net::TcpListener;
use protobuf::Message;
use std::collections::{HashMap};
use tqdc::mlink::MLinkEventHeader;

async fn events_to_point(events: Vec<MLinkEventHeader>) -> tokio::io::Result<rsb_event::Point> {

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
                frame.data.write_i16(bin).await?;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    let current_connection: Mutex<Option<JoinHandle<_>>> = Mutex::new(None);

    loop {
        let (mut socket, _) = listener.accept().await?;

        let mut current_connection_lock = current_connection.lock().await;
        if let Some(runner) = &*current_connection_lock {
            println!("new connection. aborting previous one.");
            runner.abort();
        }

        *current_connection_lock = Some(tokio::spawn(async move {
            // In a loop, read data from the socket and write the data back.
            loop {
                let msg = extract_df_message(&mut socket).await.unwrap();

                println!("{msg:?}");

                match msg.meta {
                    DFMeta::Command(command) => {
                        match command {
                            dataforge::Command::Init => {
                                push_df_message(&mut socket, DFMeta::Reply(dataforge::Reply::Init {
                                    status: dataforge::ReplyStatus::Ok,
                                    reseted: false
                                }), None).await.unwrap();
                            }
                            dataforge::Command::AcquirePoint { split: _, acquisition_time, external_meta } => {

                                let start_time = Utc::now().naive_local();
                                let events = tqdc::acquire_point(acquisition_time as u32).await.unwrap();
                                let end_time = Utc::now().naive_local();

                                let point = events_to_point(events).await.unwrap();
                                
                                let meta = DFMeta::Reply(dataforge::Reply::AcquirePoint { 
                                    acquisition_time, 
                                    start_time, 
                                    end_time, 
                                    external_meta, 
                                    status: dataforge::ReplyStatus::Ok 
                                });

                                let data = Some({
                                    let mut buf = vec![];
                                    point.write_to_vec(&mut buf).unwrap();
                                    buf
                                });

                                push_df_message(&mut socket,  meta, data).await.unwrap();
                            }
                        }
                    }

                    DFMeta::Reply(_) => {
                        todo!()
                    }
                }  
            }
        }));
    }
}
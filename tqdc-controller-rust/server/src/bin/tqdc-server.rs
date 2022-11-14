use dataforge::{extract_df_message, DFMeta, push_df_message};
use chrono::Utc;
use tokio::task::JoinHandle;
use tokio::sync::Mutex;
use tokio::net::TcpListener;
use protobuf::Message;

use server::events_to_point;

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
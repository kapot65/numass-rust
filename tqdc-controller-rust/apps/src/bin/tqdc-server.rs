use std::net::SocketAddr;

use chrono::Utc;
use clap::Parser;
use protobuf::Message;
use tokio::task::JoinHandle;
use tokio::sync::Mutex;
use tokio::net::TcpListener;
use fs2::FileExt;

use dataforge::{extract_df_message, DFMeta, push_df_message};
use apps::events_to_point;

use apps::defaults::{
    HOST_IP, HOST_CONTROL_PORT, HOST_STREAM_PORT, 
    BOARD_IP, CONTROL_PORT, STREAM_PORT
};


#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
   #[arg(long, default_value_t = HOST_IP)]
   host_ip: std::net::IpAddr,

   #[arg(long, default_value_t = 8080)]
   host_dataforge_port: u16,

   #[arg(long, default_value_t = HOST_CONTROL_PORT)]
   host_control_port: u16,

   #[arg(long, default_value_t = HOST_STREAM_PORT)]
   host_stream_port: u16,

   #[arg(long, default_value_t = BOARD_IP)]
   tqdc_ip: std::net::IpAddr,

   #[arg(long, default_value_t = CONTROL_PORT)]
   tqdc_control_port: u16,

   #[arg(long, default_value_t = STREAM_PORT)]
   tqdc_stream_port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let args = Args::parse();

    let listener = TcpListener::bind(SocketAddr::new(
        args.host_ip, args.host_dataforge_port)).await?;

    let current_connection: Mutex<Option<JoinHandle<_>>> = Mutex::new(None);

    loop {
        let (mut socket, _) = listener.accept().await?;

        let mut current_connection_lock = current_connection.lock().await;
        if let Some(runner) = &*current_connection_lock {
            println!("new connection. aborting previous one.");
            runner.abort();
        }

        *current_connection_lock = Some(tokio::spawn(async move {
            let home = home::home_dir().expect("can't obtain home directory");

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

                                let lockfile_path = home.join::<std::path::PathBuf>(".config/AFI Electronics/TQDC2/lock".into());
                                let lockfile = std::fs::OpenOptions::new().read(true).write(true).create(true).open(&lockfile_path).unwrap();
                                lockfile.lock_exclusive().unwrap();

                                let start_time = Utc::now().naive_local();
                                let events = tqdc::acquire_point(
                                    acquisition_time as u32,
                                    args.host_ip, args.host_control_port, args.host_stream_port,
                                    args.tqdc_ip, args.tqdc_control_port, args.tqdc_stream_port
                                ).await.unwrap();
                                let end_time = Utc::now().naive_local();
                                let config = Some(tqdc::get_tqdc_configuration(
                                    &home.join::<std::path::PathBuf>(".config/AFI Electronics/TQDC2/TQDC2_default.ini".into())
                                ).await);

                                let point = events_to_point(events).await.unwrap();
                                
                                let meta = DFMeta::Reply(dataforge::Reply::AcquirePoint { 
                                    acquisition_time, 
                                    start_time, 
                                    end_time, 
                                    external_meta, 
                                    config,
                                    status: dataforge::ReplyStatus::Ok 
                                });

                                let data = Some({
                                    let mut buf = vec![];
                                    point.write_to_vec(&mut buf).unwrap();
                                    buf
                                });

                                push_df_message(&mut socket,  meta, data).await.unwrap();

                                lockfile.unlock().unwrap();
                                std::fs::remove_file(lockfile_path).unwrap();
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
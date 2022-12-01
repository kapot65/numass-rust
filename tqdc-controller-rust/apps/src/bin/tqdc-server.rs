use std::net::SocketAddr;
use std::path::PathBuf;

use chrono::{Utc, Local};
use clap::Parser;
use protobuf::Message;
use tokio::task::JoinHandle;
use tokio::sync::Mutex;
use tokio::net::TcpListener;
use fs2::FileExt;

use serde_json::Value;
use eyre::{Result, ContextCompat};

use dataforge::{extract_df_message, DFMeta, push_df_message, ZeroSuppressionParams};
use apps::events_to_point;

use apps::defaults::{
    HOST_IP, HOST_CONTROL_PORT, HOST_STREAM_PORT, 
    BOARD_IP, CONTROL_PORT, STREAM_PORT
};


#[derive(Parser, Debug, Clone)]
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

   #[arg(long)]
   backup_folder: Option<PathBuf>,

   #[clap(long, short, action)]
   zero_suppression: bool,

   #[arg(long, default_value_t = 16)]
   zero_suppression_head_size: usize,

   #[arg(long, default_value_t = 34)]
   zero_suppression_threshold: i16,
}

async fn acquire_point(acquisition_time: f32, external_meta: Option<Value>, args: Args) -> Result<(DFMeta, Option<Vec<u8>>)> {

    let home = home::home_dir().wrap_err_with(|| {"unable to get home directory"})?;

    let lockfile_path = home.join::<std::path::PathBuf>(".config/AFI Electronics/TQDC2/lock".into());
    let lockfile = std::fs::OpenOptions::new().read(true).write(true).create(true).open(&lockfile_path)?;
    lockfile.lock_exclusive()?;

    let start_time = Utc::now().naive_local();
    let events = tqdc::acquire_point(
        acquisition_time as u32,
        args.host_ip, args.host_control_port, args.host_stream_port,
        args.tqdc_ip, args.tqdc_control_port, args.tqdc_stream_port
    ).await?;
    let end_time = Utc::now().naive_local();
    let config = Some(tqdc::get_tqdc_configuration(
        &home.join::<std::path::PathBuf>(".config/AFI Electronics/TQDC2/TQDC2_default.ini".into())
    ).await);

    let zero_suppression = if args.zero_suppression {
        Some(ZeroSuppressionParams {
            head_size: args.zero_suppression_head_size,
            threshold: args.zero_suppression_threshold
        })
    } else {
        None
    };

    let point = events_to_point(events, zero_suppression).await?;
    
    let meta = DFMeta::Reply(dataforge::Reply::AcquirePoint { 
        acquisition_time, 
        start_time, 
        end_time, 
        external_meta, 
        config,
        zero_suppression,
        status: dataforge::ReplyStatus::Ok 
    });

    let data = Some({
        let mut buf = vec![];
        point.write_to_vec(&mut buf)?;
        buf
    });

    lockfile.unlock()?;
    std::fs::remove_file(lockfile_path)?;

    Ok((meta, data))
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let args = Args::parse();

    let listener = TcpListener::bind(SocketAddr::new(
        args.host_ip, args.host_dataforge_port)).await?;
    println!("tqdc-server works on {}:{}", args.host_ip, args.host_dataforge_port);

    let current_connection: Mutex<Option<JoinHandle<_>>> = Mutex::new(None);

    loop {
        let (mut socket, _) = listener.accept().await?;

        let mut current_connection_lock = current_connection.lock().await;
        if let Some(runner) = &*current_connection_lock {
            println!("new connection. aborting previous one.");
            runner.abort();
        }

        let args = args.to_owned();
        *current_connection_lock = Some(tokio::spawn(async move {

            loop {

                let args = args.to_owned();
                let msg = extract_df_message(&mut socket).await
                    .expect("catch IO error on receiving DF message");

                println!("{msg:?}");

                match msg.meta {
                    DFMeta::Command(command) => {
                        match command {
                            dataforge::Command::Init => {
                                push_df_message(&mut socket, DFMeta::Reply(dataforge::Reply::Init {
                                    status: dataforge::ReplyStatus::Ok,
                                    reseted: false
                                }), None).await.expect("catch IO error on sending DF message");
                            }
                            dataforge::Command::AcquirePoint { split: _, acquisition_time, external_meta } => {
                                match acquire_point(acquisition_time, external_meta, args.clone()).await {
                                    Ok((meta, data)) => {

                                        if let Some(backup_folder) = args.backup_folder {
                                            let now = Local::now().naive_local();
                                            let filename = now.format("%Y-%m-%d-%H-%M-%S.df").to_string();
                                            let mut point_file = tokio::fs::File::create(backup_folder.join(filename))
                                                .await.expect("catch IO error on sending DF message");
                                            dataforge::push_df_message(&mut point_file, meta.clone(), data.clone())
                                                .await.expect("catch IO error on sending DF message");
                                        }

                                        push_df_message(&mut socket,  meta, data).await
                                            .expect("catch IO error on sending DF message");

                                    }
                                    Err(error) => {
                                        push_df_message(&mut socket, DFMeta::Reply(dataforge::Reply::Error { 
                                            error_code: dataforge::ErrorType::AlgoritmError, 
                                            description: error.to_string()
                                        }), None).await
                                        .expect("catch IO error on sending DF message");
                                    }
                                }
                            }
                        }
                    }

                    _ => {
                        push_df_message(&mut socket, DFMeta::Reply(dataforge::Reply::Error { 
                            error_code: dataforge::ErrorType::UnknownMessageError, 
                            description: "tqdc-server doesn't handles anything but commands".to_string()
                        }), None).await
                        .expect("catch IO error on sending DF message");
                    }
                }
            }
        }));
    }
}

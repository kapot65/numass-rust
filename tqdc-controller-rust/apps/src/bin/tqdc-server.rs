use std::net::SocketAddr;
use std::path::PathBuf;

use chrono::{Local, Utc};
use clap::Parser;
use fs2::FileExt;
use protobuf::Message;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use eyre::{ContextCompat, Report, Result};
use serde_json::Value;

use apps::events_to_point;
use dataforge::{read_df_message, write_df_message};
use numass::{NumassMeta, ZeroSuppressionParams};

use apps::defaults::{
    BOARD_IP, CONTROL_PORT, HOST_CONTROL_PORT, HOST_IP, HOST_STREAM_PORT, STREAM_PORT,
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

    /// number of first bins taken to calculate baseline
    #[arg(long, default_value_t = 16)]
    zero_suppression_baseline: usize,

    #[arg(long, default_value_t = 34)]
    zero_suppression_threshold: i16,

    #[arg(long)]
    lockfile: Option<PathBuf>,

    /// path to AFI-TQDC2 configuration file
    #[arg(long)]
    afi_config: Option<PathBuf>,
}

async fn acquire_point(
    acquisition_time: f32,
    external_meta: Option<Value>,
    args: Args,
) -> Result<(NumassMeta, Option<Vec<u8>>)> {
    let lockfile_path = if let Some(lockfile) = args.lockfile {
        lockfile
    } else {
        let home = home::home_dir().wrap_err_with(|| "unable to get home directory")?;
        home.join::<std::path::PathBuf>(".config/AFI Electronics/TQDC2/lock".into())
    };
    let lockfile = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&lockfile_path)?;
    lockfile.lock_exclusive()?;

    let start_time = Utc::now().naive_local();
    let events = tqdc::acquire_point(
        acquisition_time as u32,
        args.host_ip,
        args.host_control_port,
        args.host_stream_port,
        args.tqdc_ip,
        args.tqdc_control_port,
        args.tqdc_stream_port,
    )
    .await?;

    if events.is_empty() {
        return Err(Report::msg(
            "(acquire_point) no frames gathered from TQDC during acquisition; \
            please reset board (turn off/ turn on + fix network + restart softwafe)",
        ));
    }

    let end_time = Utc::now().naive_local();
    let config_filepath = if let Some(afi_config) = args.afi_config {
        afi_config
    } else {
        let home = home::home_dir().wrap_err_with(|| "unable to get home directory")?;
        home.join::<std::path::PathBuf>(".config/AFI Electronics/TQDC2/default/default.json".into())
    };

    let config : Option<Value>= Some(serde_json::from_reader(std::fs::File::open(config_filepath).unwrap()).unwrap());

    let zero_suppression = if args.zero_suppression {
        Some(ZeroSuppressionParams {
            baseline: args.zero_suppression_baseline,
            threshold: args.zero_suppression_threshold,
        })
    } else {
        None
    };

    let point = events_to_point(events, zero_suppression).await?;

    let meta = NumassMeta::Reply(numass::Reply::AcquirePoint {
        acquisition_time,
        start_time,
        end_time,
        external_meta,
        config,
        zero_suppression,
        status: numass::ReplyStatus::Ok,
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

    let listener =
        TcpListener::bind(SocketAddr::new(args.host_ip, args.host_dataforge_port)).await?;
    println!(
        "tqdc-server works on {}:{}",
        args.host_ip, args.host_dataforge_port
    );

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
                let msg = read_df_message(&mut socket)
                    .await
                    .expect("catch IO error on receiving DF message");

                println!("{msg:?}");

                match msg.meta {
                    NumassMeta::Command(command) => match command {
                        numass::Command::Init => {
                            write_df_message(
                                &mut socket,
                                NumassMeta::Reply(numass::Reply::Init {
                                    status: numass::ReplyStatus::Ok,
                                    reseted: false,
                                }),
                                None,
                            )
                            .await
                            .expect("catch IO error on sending DF message");
                        }
                        numass::Command::AcquirePoint {
                            split: _,
                            acquisition_time,
                            external_meta,
                        } => {
                            match acquire_point(acquisition_time, external_meta, args.clone()).await
                            {
                                Ok((meta, data)) => {
                                    if let Some(backup_folder) = args.backup_folder {
                                        let now = Local::now().naive_local();
                                        let filename =
                                            now.format("%Y-%m-%d-%H-%M-%S.df").to_string();
                                        let mut point_file =
                                            tokio::fs::File::create(backup_folder.join(filename))
                                                .await
                                                .expect("catch IO error on sending DF message");
                                        write_df_message(
                                            &mut point_file,
                                            meta.clone(),
                                            data.clone(),
                                        )
                                        .await
                                        .expect("catch IO error on sending DF message");
                                    }

                                    write_df_message(&mut socket, meta, data)
                                        .await
                                        .expect("catch IO error on sending DF message");
                                }
                                Err(error) => {
                                    write_df_message(
                                        &mut socket,
                                        NumassMeta::Reply(numass::Reply::Error {
                                            error_code: numass::ErrorType::AlgoritmError,
                                            description: error.to_string(),
                                        }),
                                        None,
                                    )
                                    .await
                                    .expect("catch IO error on sending DF message");
                                }
                            }
                        }
                    },

                    _ => {
                        write_df_message(
                            &mut socket,
                            NumassMeta::Reply(numass::Reply::Error {
                                error_code: numass::ErrorType::UnknownMessageError,
                                description: "tqdc-server doesn't handles anything but commands"
                                    .to_string(),
                            }),
                            None,
                        )
                        .await
                        .expect("catch IO error on sending DF message");
                    }
                }
            }
        }));
    }
}

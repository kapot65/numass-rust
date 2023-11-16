use std::net::SocketAddr;
use std::path::PathBuf;

use chrono::Local;
use clap::Parser;
use log::info;
use protobuf::Message;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use eyre::{ContextCompat, Report, Result};
use serde_json::Value;

use apps::events_to_point;
use dataforge::{read_df_message, write_df_message};
use processing::numass::{self, NumassMeta, ZeroSuppressionParams, ExternalMeta};
use tqdc::TQDC;

use apps::defaults::BOARD_IP;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = 8080)]
    host_dataforge_port: u16,

    #[arg(long, default_value_t = BOARD_IP)]
    tqdc_ip: std::net::IpAddr,

    #[arg(long)]
    backup_folder: Option<PathBuf>,

    #[clap(long, short, action)]
    zero_suppression: bool,

    /// number of first bins taken to calculate baseline
    #[arg(long, default_value_t = 16)]
    zero_suppression_baseline: usize,

    #[arg(long, default_value_t = 34)]
    zero_suppression_threshold: i16,

    /// FIR coefficients for zero suppression
    /// comma-separated (no spaces) list of floats (e.g. 1.0,2.0,3.0)
    #[arg(long, value_delimiter = ',')]
    zero_suppression_fir: Option<Vec<f32>>,

    /// path to AFI-TQDC2 configuration file
    #[arg(long)]
    afi_config: Option<PathBuf>,
}

/// Acquire point from TQDC and wrap it with additional metadata
async fn acquire_point(
    acquisition_time: f32,
    external_meta: Option<ExternalMeta>,
    board: &TQDC,
    args: Args,
) -> Result<(NumassMeta, Option<Vec<u8>>)> {
    
    let start_time = Local::now().naive_local();
    
    let events = board.acquire_point(acquisition_time as u32).await?;

    if events.is_empty() {
        return Err(Report::msg(
            "(acquire_point) no frames gathered from TQDC during acquisition; \
            please reset board (turn off/ turn on + fix network + restart softwafe)",
        ));
    }

    let end_time = Local::now().naive_local();
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
            fir: args.zero_suppression_fir,
        })
    } else {
        None
    };

    let point = events_to_point(events, zero_suppression.clone()).await?;

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

    Ok((meta, data))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    env_logger::init();

    let args = Args::parse();

    let listener =
        TcpListener::bind(SocketAddr::new([0,0,0,0].into(), args.host_dataforge_port)).await?;

    info!("tqdc-server works on 0.0.0.0:{}", args.host_dataforge_port);

    let current_connection: Mutex<Option<JoinHandle<_>>> = Mutex::new(None);

    loop {
        let (mut socket, _) = listener.accept().await?;

        let mut current_connection_lock = current_connection.lock().await;
        if let Some(runner) = &*current_connection_lock {
            info!("new connection. aborting previous one.");
            runner.abort();
        }

        let args = args.to_owned();
        *current_connection_lock = Some(tokio::spawn(async move {
            loop {
                let args = args.to_owned();
                let board = TQDC::new(args.tqdc_ip);
                let msg = read_df_message(&mut socket)
                    .await
                    .expect("catch IO error on receiving DF message");

                info!("received message: {msg:?}");

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
                            path: _,
                            external_meta,
                        } => {
                            match acquire_point(
                                acquisition_time, 
                                external_meta, 
                                &board,
                                args.clone()
                            ).await
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

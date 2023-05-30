pub mod mlink;
pub mod regs;

use std::net::{IpAddr, SocketAddr};
use std::{path::PathBuf, vec};

use eyre::{Context, ContextCompat, Report, Result};

use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio::net::UdpSocket;
use tokio::time::{self, sleep, Duration};

use mlink::{CtrlReg, MStreamFragment, MlinkMessage};
use regs::{as_run_state, Register16, Register32, RunMode, RunState, RunLogicControl};

async fn start_acquisition(
    millis: u32,
    host_control_addr: SocketAddr,
    tqdc_contol_addr: SocketAddr,
) -> Result<()> {
    let sock = UdpSocket::bind(host_control_addr).await?;
    
    sock.send_to(
        &MlinkMessage::to_datagram(&MlinkMessage::new_ctrl_req(
            0x0000,
            0x0001,
            0xFEFE,
            vec![
                
            CtrlReg::Write16 {
                address: Register16::RunLogicControl,
                value: RunLogicControl::SoftClear as u16,
            }, 
            CtrlReg::Write16 {
                address: Register16::RunLogicControl,
                value: RunLogicControl::Stop as u16,
            }, 
            
            CtrlReg::Write16 {
                address: Register16::RunLogicRunMode,
                value: RunMode::Time as u16,
            },
            CtrlReg::Write32 {
                address: Register32::RunTimeLimit,
                value: millis,
            },
            
            
            CtrlReg::Write16 {
                address: Register16::RunLogicControl,
                value: RunLogicControl::SoftClear as u16,
            }, 
            CtrlReg::Write16 {
                address: Register16::RunLogicControl,
                value: RunLogicControl::Stop as u16,
            },
            
            
            CtrlReg::Write16 {
                address: Register16::RunLogicControl,
                value: RunLogicControl::Run as u16,
            },
            ],
        )),
        tqdc_contol_addr,
    )
    .await?;

    let mut buf = [0; 1536];

    match time::timeout(Duration::from_millis(100), sock.recv_from(&mut buf)).await {
        Ok(res) => match res {
            Ok((_, _)) => Ok(()),
            Err(err) => Err(err.into()),
        },
        Err(_) => Err(Report::msg("(start_acquisition) - acq timeout")),
    }
}

async fn check_run_state(
    host_control_addr: SocketAddr,
    tqdc_contol_addr: SocketAddr,
) -> Result<RunState> {
    let sock = UdpSocket::bind(host_control_addr).await?;

    sock.send_to(
        &MlinkMessage::to_datagram(&MlinkMessage::new_ctrl_req(
            0x0000,
            0x0001,
            0xFEFE,
            vec![CtrlReg::Read16 {
                address: Register16::RunLogicRunState,
                value: 0x0000,
            }],
        )),
        tqdc_contol_addr,
    )
    .await?;

    let mut buf = [0; 1536 * 10];

    match time::timeout(Duration::from_millis(100), sock.recv_from(&mut buf)).await {
        Ok(res) => match res {
            Ok((len, _)) => {
                let acq = MlinkMessage::from_datagram(&buf[..len]);
                match acq {
                        MlinkMessage::CtrlAck { header: _, regs } => {
                            if regs.len() != 1 {
                                Err(Report::msg(format!(
                                    "(check_run_state) - excepted CtrlAck message with single Read16(RunState) command, found: {regs:?}")))
                            } else {
                                match &regs[0] {
                                    CtrlReg::Read16 { address, value } => {
                                        match address {
                                            Register16::RunLogicRunState => Ok(as_run_state(*value).wrap_err_with(|| {"as_run_state failed"})?),
                                            _ => Err(Report::msg(format!(
                                                "(check_run_state) - excepted CtrlAck message with single Read16(RunState) command, found: {address:?}")))
                                        }
                                    }
                                    other => Err(Report::msg(format!(
                                        "(check_run_state) - excepted CtrlAck message with single Read16(RunState) command, found: {other:?}")))
                                }
                            }
                        }
                        other => Err(Report::msg(format!(
                            "(check_run_state) - excepted CtrlAck message with single Read16(RunState) command, found: {other:?}")))
                    }
            }
            Err(err) => Err(err.into()),
        },
        Err(_) => Err(Report::msg("(check_run_state) - acq timeout")),
    }
}

async fn stop_acquisition(
    host_control_addr: SocketAddr,
    tqdc_contol_addr: SocketAddr,
) -> Result<()> {
    let sock = UdpSocket::bind(host_control_addr).await?;

    sock.send_to(
        &MlinkMessage::to_datagram(&MlinkMessage::new_ctrl_req(
            0x0000,
            0x0001,
            0xFEFE,
            vec![CtrlReg::Write16 {
                address: Register16::RunLogicControl,
                value: RunLogicControl::Stop as u16,
            }],
        )),
        tqdc_contol_addr,
    )
    .await?;

    let mut buf = [0; 4096 * 10];
    match time::timeout(Duration::from_millis(100), sock.recv_from(&mut buf)).await {
        Ok(res) => match res {
            Ok((_, _)) => Ok(()),
            Err(err) => Err(err.into()),
        },
        Err(_) => Err(Report::msg("(stop_acquisition) - acq timeout")),
    }
}

async fn gather_frames(
    host_control_addr: SocketAddr,
    tqdc_contol_addr: SocketAddr,
    host_stream_addr: SocketAddr,
    tqdc_stream_addr: SocketAddr,
) -> Result<Vec<[u8; 1456]>> {
    let mut frames = Vec::with_capacity(30_000 * 100);
    let stream_socket = UdpSocket::bind(host_stream_addr).await?;

    stream_socket
        .send_to(
            &MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
                0x0000, 0x0101, 0x0001, 0xFFFF, 0xFFFF,
            )),
            tqdc_stream_addr,
        )
        .await?;

    loop {
        let mut buf = [0; 1456];

        match tokio::time::timeout(
            Duration::from_millis(500),
            stream_socket.recv_from(&mut buf),
        ).await
        {
            Ok(received) => {
                let message = MlinkMessage::from_datagram(&buf[..received?.0]);
                
                if let MlinkMessage::StreamReq { header, frames } = message {
                    let frame = frames.first().unwrap();
                    stream_socket
                    .send_to(
                        // &stream_to_acq_fast(&buf)
                        &MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
                                header.seq,
                                0x0001,
                                0xfefe,
                                frame.fragment_offset,
                                frame.fragment_id,
                        ))
                        , 
                        tqdc_stream_addr)
                    .await?;
                }
                frames.push(buf);
            }
            Err(_) => {
                let state = check_run_state(host_control_addr, tqdc_contol_addr).await?;

                println!("state: {state:?}");
                
                match state {
                    RunState::Finished => break,
                    RunState::InRun => {}
                    RunState::Stopped => Err(Report::msg("run state = Stopped"))?,
                }
            }
        }
    }
    Ok(frames)
}

fn frames_to_events(frames: Vec<[u8; 1456]>) -> Result<Vec<MStreamFragment>> {
    let mut events = vec![];
    for frame in frames {
        let message = MlinkMessage::from_datagram(&frame);
        match message {
            MlinkMessage::StreamReq { header: _, frames } => {
                if frames.first().is_some() {
                    events.extend(frames);
                } else {
                    Err(Report::msg(
                        "(gather_frames) - incoming stream packet has no frames",
                    ))?
                }
            }
            _ => Err(Report::msg(
                "(gather_frames) - incoming packet is not STREAM type",
            ))?,
        }
    }
    Ok(events)
}

pub async fn acquire_point(
    acquisition_time_s: u32,
    host_ip: IpAddr,
    host_control_port: u16,
    host_stream_port: u16,
    tqdc_ip: IpAddr,
    tqdc_control_port: u16,
    tqdc_stream_port: u16,
) -> Result<Vec<MStreamFragment>> {
    let acquisition_time_ms = acquisition_time_s * 1000;

    let host_control_address = SocketAddr::new(host_ip, host_control_port);
    let host_stream_address = SocketAddr::new(host_ip, host_stream_port);

    let tqdc_control_address = SocketAddr::new(tqdc_ip, tqdc_control_port);
    let tqdc_stream_address = SocketAddr::new(tqdc_ip, tqdc_stream_port);

    let gather_loop = tokio::spawn(gather_frames(
        host_control_address,
        tqdc_control_address,
        host_stream_address,
        tqdc_stream_address,
    ));

    sleep(Duration::from_millis(100)).await;
    start_acquisition(
        acquisition_time_ms,
        host_control_address,
        tqdc_control_address,
    )
    .await?;

    let frames = (gather_loop.await.with_context(|| "gather_loop ")?)?;

    stop_acquisition(host_control_address, tqdc_control_address).await?;

    let events = frames_to_events(frames)?;
    Ok(events)
}

#[deprecated(since="0.1.0", note="new tqdc board saves configuration in json format")]
pub async fn get_tqdc_configuration(path_to_config: &PathBuf) -> Value {
    let mut file = tokio::fs::File::open(path_to_config)
        .await
        .expect("cant open configuation file");
    let mut str = String::new();
    file.read_to_string(&mut str).await.unwrap();

    let mut lines = str.lines();

    let current_device_flag = "current_device_";

    let device_id =
        if let Some(current_device_line) = lines.find(|l| l.contains(current_device_flag)) {
            let pos = current_device_line.find(current_device_flag).unwrap();
            String::from(
                &current_device_line
                    [pos + current_device_flag.len()..pos + current_device_flag.len() + 9],
            )
        } else {
            panic!()
        };

    let prefix = format!("default\\default\\known_setups\\device_{device_id}\\");
    let config_lines = lines
        .filter(|line| line.starts_with(&prefix))
        .map(|line| String::from(&line[prefix.len()..]))
        .collect::<Vec<_>>();

    let mut config = json!({});

    for config_line in config_lines {
        let (path, value) = {
            let splitted = config_line.split('=').collect::<Vec<_>>();
            (String::from(splitted[0]), String::from(splitted[1]))
        };

        let mut entry = config.as_object_mut().unwrap();
        let keys = path.split('\\').map(String::from).collect::<Vec<_>>();
        for key in keys[..keys.len() - 1].iter() {
            entry = entry
                .entry(key)
                .or_insert(json!({}))
                .as_object_mut()
                .unwrap()
        }

        if value.eq("null") {
            entry.insert(keys.last().unwrap().clone(), serde_json::Value::Null);
        } else if let Ok(value) = value.parse::<bool>() {
            entry.insert(keys.last().unwrap().clone(), serde_json::Value::Bool(value));
        } else if let Ok(value) = value.parse::<i64>() {
            entry.insert(keys.last().unwrap().clone(), json!(value));
        } else if let Ok(value) = value.parse::<f32>() {
            entry.insert(keys.last().unwrap().clone(), json!(value));
        } else {
            entry.insert(
                keys.last().unwrap().clone(),
                serde_json::Value::String(value),
            );
        }
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_config() {
        let path_to_config = if let Some(home) = home::home_dir() {
            home.join::<PathBuf>(".config/AFI Electronics/TQDC2/TQDC2_default.ini".into())
        } else {
            panic!("can't obtain home directory")
        };
        let conf = get_tqdc_configuration(&path_to_config).await;
        println!("{}", serde_json::to_string_pretty(&conf).unwrap())
    }
}

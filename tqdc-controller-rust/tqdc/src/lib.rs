pub mod mlink;
pub mod regs;

use std::net::{IpAddr, SocketAddr};
use std::{vec, path::PathBuf};

use tokio::io;
use tokio::io::AsyncReadExt;
use tokio::net::UdpSocket;
use tokio::time::{sleep, Duration};
use serde_json::{Value, json};

use regs::{Register16, Register32, RunMode, DeviceCtrl, TriggerCSR, RunState, as_run_state};
use mlink::{MlinkMessage, CtrlReg, MStreamFragment};

async fn start_acquisition(millis: u32, host_control_addr: SocketAddr, tqdc_contol_addr: SocketAddr) -> io::Result<()> {
    let sock = UdpSocket::bind(host_control_addr).await?;

    sock.send_to(&MlinkMessage::to_datagram(&MlinkMessage::new_ctrl_req(
        0x0000, 
        0x0001, 
        0xFEFE, 
        vec![
            CtrlReg::Write32 {
                address: Register32::TriggerEventNumLoad,
                value: 0
            },
            CtrlReg::Write16 {
                address: Register16::TriggerCSR,
                value: TriggerCSR::CountReset as u16
            },
            CtrlReg::Write16 {
                address: Register16::TriggerCSR,
                value: TriggerCSR::Exec as u16
            },
            CtrlReg::Write32 { 
                address: Register32::RunMode, 
                value: RunMode::Time as u32 
            },
            CtrlReg::Write32 {
                address: Register32::TimeLimit,
                value: millis
            },
            CtrlReg::Write16 {
                address: Register16::DeviceCtrl, 
                value: DeviceCtrl::Run as u16
            }
        ]
    )), tqdc_contol_addr).await?;

    let mut buf = [0; 4096 * 10];
    sock.recv_from(&mut buf).await?;
    // let (len, _) = sock.recv_from(&mut buf).await?;
    // let acq = MlinkMessage::from_datagram(&buf[..len]);
    // TODO: add correctness checking

    Ok(())
}

async fn check_run_state(host_control_addr: SocketAddr, tqdc_contol_addr: SocketAddr) -> io::Result<RunState> {
    let sock = UdpSocket::bind(host_control_addr).await?;

    sock.send_to(&MlinkMessage::to_datagram(&MlinkMessage::new_ctrl_req(
        0x0000, 
        0x0001, 
        0xFEFE, 
        vec![
            CtrlReg::Read16 { 
                address: Register16::RunState, 
                value: 0x0000
            }
        ]
    )), tqdc_contol_addr).await?;

    let mut buf = [0; 4096 * 10];
    let (len, _) = sock.recv_from(&mut buf).await?;
    let acq = MlinkMessage::from_datagram(&buf[..len]);

    match acq {
        MlinkMessage::CtrlAck { header: _, regs } => {
            if regs.len() != 1 { panic!("excepted CtrlAck message with single Read16(RunState) command, found: {:?}", regs) }
            match &regs[0] {
                CtrlReg::Read16 { address, value } => {
                    match address {
                        Register16::RunState => Ok(as_run_state(*value).unwrap()),
                        _ => panic!("excepted CtrlAck message with single Read16(RunState) command, found: {:?}", address)
                    }
                }
                other => panic!("excepted CtrlAck message with single Read16(RunState) command, found: {:?}", other)
            }
        }
        other => panic!("excepted CtrlAck message with single Read16(RunState) command, found: {:?}", other)
    }
}

async fn stop_acquisition(host_control_addr: SocketAddr, tqdc_contol_addr: SocketAddr) -> io::Result<()> {
    let sock = UdpSocket::bind(host_control_addr).await?;

    sock.send_to(&MlinkMessage::to_datagram(&MlinkMessage::new_ctrl_req(
        0x0000, 
        0x0001, 
        0xFEFE, 
        vec![
            CtrlReg::Write16 {
                address: Register16::DeviceCtrl, 
                value: DeviceCtrl::Stop as u16
            }
        ]
    )), tqdc_contol_addr).await?;
    
    let mut buf = [0; 4096 * 10];
    sock.recv_from(&mut buf).await?;
    // let (len, _) = sock.recv_from(&mut buf).await?;
    // let acq = MlinkMessage::from_datagram(&buf[..len]);
    // TODO: add correctness checking

    Ok(())
}

async fn gather_frames(
    host_control_addr: SocketAddr, 
    tqdc_contol_addr: SocketAddr, 
    host_stream_addr: SocketAddr, 
    tqdc_stream_addr: SocketAddr
) -> io::Result<Vec<MStreamFragment>> {

    let mut events = Vec::new();
    let stream_socket = UdpSocket::bind(host_stream_addr).await?;

    stream_socket.send_to(&MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
        0x0000, 
        0x0101, 
        0x0001, 
        0xFFFF, 
        0xFFFF
    )), tqdc_stream_addr).await?;


    loop {
        let mut buf = [0; 1448];

        match tokio::time::timeout(
            Duration::from_millis(500), 
            stream_socket.recv_from(&mut buf)).await {
            Ok(received) => {
                let (len, _) = received?;
                let message = MlinkMessage::from_datagram(&buf[..len]);

                match message {
                    MlinkMessage::StreamReq { header, frames } => {
                        if let Some(frame) = frames.first() {

                            stream_socket.send_to(&MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
                                header.seq, 
                                0x0101, 
                                0x0001, 
                                frame.fragment_offset, 
                                frame.fragment_id, 
                            )), tqdc_stream_addr).await?;

                            events.extend(frames);

                        } else {
                            panic!("no frames in package!")
                        }
                    }
                    _ => panic!("unimplemented!!")
                }
            }
            Err(_) => {
                let state = check_run_state(
                    host_control_addr, 
                    tqdc_contol_addr
                ).await.unwrap();

                match state {
                    RunState::Finished => break,
                    RunState::InRun => {}
                    RunState::Stopped => panic!("TODO: add panic")
                }
            }
        }
    };

    Ok(events)
}

pub async fn acquire_point(
    acquisition_time_s: u32, 
    host_ip: IpAddr,
    host_control_port: u16,
    host_stream_port: u16,
    tqdc_ip: IpAddr,
    tqdc_control_port: u16,
    tqdc_stream_port: u16
) -> io::Result<Vec<MStreamFragment>> {

    let acquisition_time_ms = acquisition_time_s * 1000;

    
    let host_control_address = SocketAddr::new(host_ip, host_control_port);
    let host_stream_address = SocketAddr::new(host_ip, host_stream_port);

    let tqdc_control_address = SocketAddr::new(tqdc_ip, tqdc_control_port);
    let tqdc_stream_address = SocketAddr::new(tqdc_ip, tqdc_stream_port);


    {
        tokio::spawn(async move{
            sleep(Duration::from_millis(200)).await;
            start_acquisition(
                acquisition_time_ms, 
                host_control_address,
                tqdc_control_address
            ).await.unwrap();
        });
    }
    
    let events = gather_frames(
        host_control_address,
        tqdc_control_address,
        host_stream_address,
        tqdc_stream_address
    ).await?;

    stop_acquisition(
        host_control_address, 
        tqdc_control_address
    ).await?;

    Ok(events)
}

pub async fn get_tqdc_configuration(path_to_config: &PathBuf) -> Value {

    let mut file = tokio::fs::File::open(path_to_config).await
        .expect("cant open configuation file");
    let mut str = String::new();
    file.read_to_string(&mut str).await.unwrap();


    let mut lines = str.lines();

    let current_device_flag = "current_device_";

    let device_id = if let Some(current_device_line) = lines.find(|l| l.contains(current_device_flag)) {
        let pos = current_device_line.find(current_device_flag).unwrap();
        String::from(&current_device_line[pos+current_device_flag.len()..pos+current_device_flag.len()+9])  
    } else {
        panic!()
    };

    let prefix = format!("default\\default\\known_setups\\device_{device_id}\\");
    let config_lines = lines.filter(|line| {line.starts_with(&prefix)}).map(|line| {
        String::from(&line[prefix.len()..])
    }).collect::<Vec<_>>();


    let mut config = json!({});
    
    for config_line in config_lines {

        let (path, value) = {
            let splitted = config_line.split('=').collect::<Vec<_>>();
            (String::from(splitted[0]), String::from(splitted[1]))
        };

        let mut entry = config.as_object_mut().unwrap();
        let keys = path.split('\\').map(|s| {String::from(s)}).collect::<Vec<_>>();
        for key in keys[..keys.len() - 1].iter() {
            entry = entry.entry(key).or_insert(json!({})).as_object_mut().unwrap()
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
            entry.insert(keys.last().unwrap().clone(), serde_json::Value::String(value));
        }
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_config() {
        let path_to_config =  if let Some(home) = home::home_dir() {
            home.join::<PathBuf>(".config/AFI Electronics/TQDC2/TQDC2_default.ini".into())
        } else {
            panic!("can't obtain home directory")
        };
        let conf = get_tqdc_configuration(&path_to_config).await;
        println!("{}", serde_json::to_string_pretty(&conf).unwrap())
    }
}
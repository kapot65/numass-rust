pub mod mlink;
pub mod regs;


use {mlink::{MlinkMessage, CtrlReg}, regs::{RunState, as_run_state}};
use mlink::MLinkEventHeader;
use tokio::io;
use std::{net::SocketAddrV4, collections::{HashMap, VecDeque}, vec};
use tokio::net::UdpSocket;
use tokio::time::{sleep, Duration};
use regs::{Register16, Register32, RunMode, DeviceCtrl, TriggerCSR};

use dataforge::{protos::rsb_event::{self, Point}, DFMessage};
use tokio::io::AsyncWriteExt;

// TODO: move to config
pub const HOST_IP: &str = "0.0.0.0";
pub const BOARD_IP: &str = "10.0.0.5";
pub const CONTROL_PORT: u16 = 33300;
pub const STREAM_PORT: u16 = 33301;


async fn start_acquisition(millis: u32) -> io::Result<()> {
    let sock = UdpSocket::bind(format!("{HOST_IP}:{CONTROL_PORT}")).await?;
    let to_addr =  format!("{BOARD_IP}:{CONTROL_PORT}").parse::<SocketAddrV4>().unwrap();

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
    )), to_addr).await?;

    let mut buf = [0; 4096 * 10];
    let (len, _) = sock.recv_from(&mut buf).await?;
    let acq = MlinkMessage::from_datagram(&buf[..len]);
    
    // TODO: add correctness checking

    Ok(())
}

async fn check_run_state() -> io::Result<RunState> {
    let sock = UdpSocket::bind(format!("{HOST_IP}:{CONTROL_PORT}")).await?;
    let to_addr = format!("{BOARD_IP}:{CONTROL_PORT}").parse::<SocketAddrV4>().unwrap();

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
    )), to_addr).await?;

    let mut buf = [0; 4096 * 10];
    let (len, _) = sock.recv_from(&mut buf).await?;
    let acq = MlinkMessage::from_datagram(&buf[..len]);

    match acq {
        MlinkMessage::CtrlAck { header, regs } => {
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

async fn stop_acquisition() -> io::Result<()> {
    let sock = UdpSocket::bind(format!("{HOST_IP}:{CONTROL_PORT}")).await?;
    let to_addr = format!("{BOARD_IP}:{CONTROL_PORT}").parse::<SocketAddrV4>().unwrap();

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
    )), to_addr).await?;
    
    let mut buf = [0; 4096 * 10];
    let (len, _) = sock.recv_from(&mut buf).await?;

    let acq = MlinkMessage::from_datagram(&buf[..len]);

    // TODO: add correctness checking

    Ok(())
}


async fn gather_frames(acquisition_time_ms: u32) -> io::Result<Vec<MLinkEventHeader>> {

    let mut events = Vec::new();

    let stream_socket = UdpSocket::bind(format!("{HOST_IP}:{STREAM_PORT}")).await?;
    let stream_addr = format!("{BOARD_IP}:{STREAM_PORT}").parse::<SocketAddrV4>().unwrap();

    stream_socket.send_to(&MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
        0x0000, 
        0x0101, 
        0x0001, 
        0xFFFF, 
        0xFFFF
    )), stream_addr).await?;


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
                            )), stream_addr).await?;

                            events.extend(frames);

                        } else {
                            panic!("no frames in package!")
                        }
                    }
                    _ => panic!("unimplemented!!")
                }
            }
            Err(_) => {
                let state = check_run_state().await.unwrap();

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

pub async fn acquire_point(acquisition_time_s: u32) -> io::Result<Vec<MLinkEventHeader>> {

    let acquisition_time_ms = acquisition_time_s * 1000;

    tokio::spawn(async move{
        sleep(Duration::from_millis(200)).await;
        start_acquisition(acquisition_time_ms).await.unwrap();
    });
    
    let events = gather_frames(acquisition_time_ms).await?;

    stop_acquisition().await?;

    Ok(events)
}
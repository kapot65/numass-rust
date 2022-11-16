use tokio::io;
use tqdc::config::{HOST_IP, CONTROL_HOST_PORT, BOARD_IP, CONTROL_PORT};
use std::net::SocketAddrV4;
use tokio::net::UdpSocket;

use tqdc::regs::Register16;
use tqdc::mlink::{MlinkMessage, CtrlReg};

#[tokio::main]
async fn main() -> io::Result<()> {

    let msg = MlinkMessage::new_ctrl_req(
        0x0000, 
        0x0001, 
        0xFEFE, 
        vec![
            CtrlReg::Read16 { 
                address: Register16::DeviceId, 
                value: 0x0000 
            }
        ]
    );

    // HOST_IP, CONTROL_HOST_PORT

    let sock = UdpSocket::bind(format!("{HOST_IP}:{CONTROL_HOST_PORT}")).await?;
    let to_addr = format!("{BOARD_IP}:{CONTROL_PORT}").parse::<SocketAddrV4>().unwrap();

    sock.send_to(&MlinkMessage::to_datagram(&msg), to_addr).await?;

    let mut buf = [0; 4096 * 10];
    let (len, _) = sock.recv_from(&mut buf).await?;


    let acq = MlinkMessage::from_datagram(&buf[..len]);

    println!("{:?}", acq);

    Ok(())
}
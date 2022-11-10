use tokio::io;
use std::net::SocketAddrV4;
use tokio::net::UdpSocket;

use tqdc::regs::{Register16, Register32};
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

    let sock = UdpSocket::bind("0.0.0.0:33300").await?;
    let to_addr = "10.0.0.5:33300".parse::<SocketAddrV4>().unwrap();

    sock.send_to(&MlinkMessage::to_datagram(&msg), to_addr).await?;

    let mut buf = [0; 4096 * 10];
    let (len, _) = sock.recv_from(&mut buf).await?;


    let acq = MlinkMessage::from_datagram(&buf[..len]);

    println!("{:?}", acq);

    Ok(())
}
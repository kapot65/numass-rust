use std::net::SocketAddr;
use tokio::io;
use tokio::net::UdpSocket;

use clap::Parser;

use apps::defaults::BOARD_IP;
use tqdc::TQDC_CONTROL_PORT;
use tqdc::mlink::{CtrlReg, MlinkMessage};
use tqdc::regs::Register16;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = BOARD_IP)]
    tqdc_ip: std::net::IpAddr,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();

    let msg = MlinkMessage::new_ctrl_req(
        0x0000,
        0x0001,
        0xFEFE,
        vec![CtrlReg::Read16 {
            address: Register16::DeviceId,
            value: 0x0000,
        }],
    );

    let tqdc_address = SocketAddr::new(args.tqdc_ip, TQDC_CONTROL_PORT);

    let sock = UdpSocket::bind("0.0.0.0:0").await?;

    sock.send_to(&MlinkMessage::to_datagram(&msg), tqdc_address)
        .await?;

    let mut buf = [0; 4096 * 10];
    let (len, _) = sock.recv_from(&mut buf).await?;

    let acq = MlinkMessage::from_datagram(&buf[..len]);

    println!("{acq:?}");

    Ok(())
}

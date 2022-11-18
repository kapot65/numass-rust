use tokio::io;
use std::net::SocketAddr;
use tokio::net::UdpSocket;

use clap::Parser; 

use tqdc::regs::Register16;
use tqdc::mlink::{MlinkMessage, CtrlReg};
use apps::defaults::{HOST_IP, HOST_CONTROL_PORT, BOARD_IP, CONTROL_PORT};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
   #[arg(long, default_value_t = HOST_IP)]
   host_ip: std::net::IpAddr,

   #[arg(long, default_value_t = HOST_CONTROL_PORT)]
   host_control_port: u16,

   #[arg(long, default_value_t = BOARD_IP)]
   tqdc_ip: std::net::IpAddr,

   #[arg(long, default_value_t = CONTROL_PORT)]
   tqdc_control_port: u16,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    
    let args = Args::parse();

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

    let host_address = SocketAddr::new(args.host_ip, args.host_control_port);
    let tqdc_address = SocketAddr::new(args.tqdc_ip, args.tqdc_control_port);

    let sock = UdpSocket::bind(host_address).await?;

    sock.send_to(&MlinkMessage::to_datagram(&msg), tqdc_address).await?;

    let mut buf = [0; 4096 * 10];
    let (len, _) = sock.recv_from(&mut buf).await?;

    let acq = MlinkMessage::from_datagram(&buf[..len]);

    println!("{:?}", acq);

    Ok(())
}
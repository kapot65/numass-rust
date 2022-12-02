use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::net::{SocketAddr};
use std::time::Duration;

use tokio::io;
use tokio::io::AsyncReadExt;
use tokio::sync::{Mutex, watch};
use tokio::net::UdpSocket;
use tokio_timerfd::sleep;

use tqdc::regs::{Register16, Register32, DeviceCtrl};
use tqdc::mlink::{MlinkMessage, CtrlReg};
use apps::defaults::{BOARD_IP, CONTROL_PORT, STREAM_PORT};

async fn handle_control_port(
    regs16: Arc<Mutex<HashMap<u16, u16>>>,
    regs32: Arc<Mutex<HashMap<u16, u32>>>,
    control_tx: watch::Sender<()>
) -> io::Result<()> {
    let sock = UdpSocket::bind(
        format!("{BOARD_IP}:{CONTROL_PORT}")).await?;

    loop {
        let mut buf = [0; 1536];
        let (len, to_addr) = sock.recv_from(&mut buf).await?;

        let msg = MlinkMessage::from_datagram(&buf[..len]);
    
        println!("{msg:?}");

        match msg {
            MlinkMessage::CtrlReq { header, regs } => {
                
                let mut acq = Vec::with_capacity(regs.len());

                for reg in regs {
                    acq.push(match reg {
                        CtrlReg::Read16 { address, value: _ } => {
                            if let Some(rval) = regs16.lock().await.get(&(address as u16)) {
                                CtrlReg::Read16 { address, value: *rval }
                            } else {
                                panic!("adress {address:x?} not found in registers")
                            }
                        }
                        CtrlReg::Write16 { address, value } => {
                            regs16.lock().await.insert(address as u16, value);
                        
                            if let Register16::DeviceCtrl = address {
                                if value == DeviceCtrl::Run as u16 {
                                    control_tx.send(()).unwrap();
                                }
                            }

                            CtrlReg::Write16 { address, value }
                        }
                        CtrlReg::Write32 { address, value } => {
                            regs32.lock().await.insert(address as u16, value);
                            CtrlReg::Write32 { address, value }
                        }
                        other => todo!("handing for {other:?}")
                    })
                }

                let msg = MlinkMessage::new_ctrl_acq(
                    header.seq, header.dst, header.src, acq);
                sock.send_to(&MlinkMessage::to_datagram(&msg), to_addr).await?;

            }
            
            other => panic!("unimplemented behaviour for {other:?}")
        }
    }
}

async fn handle_stream_port(
    regs32: Arc<Mutex<HashMap<u16, u32>>>,
    mut control_rx: watch::Receiver<()>
) -> io::Result<()> {

    let to_addr_rx = Arc::new(Mutex::new(SocketAddr::from_str("0.0.0.0:3330").unwrap()));
    let to_addr_tx = Arc::clone(&to_addr_rx);
    
    let rx = Arc::new(UdpSocket::bind(
        format!("{BOARD_IP}:{STREAM_PORT}")).await?);
    let tx = Arc::clone(&rx);


    let rx_handler = tokio::spawn(async move {
        loop {
            let mut buf = [0; 1536];
            let (_, addr) = rx.recv_from(&mut buf).await.unwrap();
    
            let mut lock = to_addr_rx.lock().await;
            *lock = addr;
            // let msg = MlinkMessage::from_datagram(&buf[..len]);
        }
    });

    let tx_handler = tokio::spawn(async move {

        let mut file = tokio::fs::File::open(
            "./test-data/frames-408ns-7ch.bin"
        ).await.unwrap();
        let mut contents = [0u8; 2048];
        let size = file.read(&mut contents).await.unwrap();

        loop  {
            control_rx.changed().await.unwrap();
            let to_addr = *to_addr_tx.lock().await;

            let time_limit = match regs32.lock().await.get(&(Register32::TimeLimit as u16))   {
                Some(time_limit) => *time_limit,
                None => panic!("time_limit register not found")
            };

            tokio::time::timeout(Duration::from_millis(time_limit as u64), async {
                loop {
                    sleep(Duration::from_micros(10)).await.unwrap();
                    tx.send_to(&contents[..size], to_addr).await.unwrap();
                }
            }).await.unwrap_err();     
        }
    });

    let(rx_res, tx_res) = tokio::join!(rx_handler, tx_handler);
    rx_res?;
    tx_res?;
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {

    let (control_tx, control_rx) = watch::channel(());
    
    let regs16 = Arc::new(Mutex::new(HashMap::new()));
    let regs32: Arc<Mutex<HashMap<u16, u32>>> = Arc::new(Mutex::new(HashMap::new()));
    
    {
      let mut guard =  regs16.lock().await;
      guard.insert(Register16::DeviceId as u16, 0x0001);
      guard.insert(
        Register16::RunState as u16, 
        tqdc::regs::RunState::Finished as u16);
    }

    let control_listener = tokio::spawn(
        handle_control_port(
            Arc::clone(&regs16), 
            Arc::clone(&regs32),
            control_tx
        ));

    let stream_listener = tokio::spawn(
        handle_stream_port(
            Arc::clone(&regs32),
            control_rx
        ));

    let (control_res, stream_res) = tokio::join!(control_listener, stream_listener);

    (control_res?)?;
    (stream_res?)?;

    Ok(())
}
pub mod mlink;
pub mod mstream;
pub mod regs;

use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::vec;

use eyre::{Context, ContextCompat, Report, Result};

use tokio::net::UdpSocket;
use tokio::time::{self, sleep, Duration};

use mlink::{CtrlReg, MlinkMessage};
use mstream::{MStreamTriggerAndUserData, MStreamADCBlocks};
use regs::{as_run_state, Register16, Register32, RunMode, RunState, RunLogicControl};

pub const MTU_SIZE: usize = 1536;

pub const TQDC_CONTROL_PORT: u16 = 33300;
pub const TQDC_STREAM_PORT: u16 = 33301;

#[derive(Debug, Copy, Clone)]
pub struct TQDC {
    tqdc_control_addr: SocketAddr,
    tqdc_stream_addr: SocketAddr,
}

impl TQDC {

    pub fn new<A: Into<IpAddr> + Copy>(tqdc_addr: A) -> Self {
        let tqdc_control_addr = SocketAddr::new(tqdc_addr.into(), TQDC_CONTROL_PORT);
        let tqdc_stream_addr = SocketAddr::new(tqdc_addr.into(), TQDC_STREAM_PORT);
        TQDC { 
            tqdc_control_addr, 
            tqdc_stream_addr
        }
    }

    async fn start_acquisition(&self, millis: u32, control: &UdpSocket) -> Result<()>  {
        control.send_to(
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
            self.tqdc_control_addr,
        )
        .await?;
    
        let mut buf = [0; MTU_SIZE];
    
        match time::timeout(Duration::from_millis(100), control.recv_from(&mut buf)).await {
            Ok(res) => match res {
                Ok((_, _)) => Ok(()),
                Err(err) => Err(err.into()),
            },
            Err(_) => Err(Report::msg("(start_acquisition) - acq timeout")),
        }
    }

    async fn check_run_state(
        &self,
        control: &UdpSocket,
    ) -> Result<RunState> {
        control.send_to(
            &MlinkMessage::to_datagram(&MlinkMessage::new_ctrl_req(
                0x0000,
                0x0001,
                0xFEFE,
                vec![CtrlReg::Read16 {
                    address: Register16::RunLogicRunState,
                    value: 0x0000,
                }],
            )),
            self.tqdc_control_addr,
        )
        .await?;
    
        let mut buf = [0; MTU_SIZE * 10];
    
        match time::timeout(Duration::from_millis(100), control.recv_from(&mut buf)).await {
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
        &self,
        control: &UdpSocket,
    ) -> Result<()> {
        control.send_to(
            &MlinkMessage::to_datagram(&MlinkMessage::new_ctrl_req(
                0x0000,
                0x0001,
                0xFEFE,
                vec![CtrlReg::Write16 {
                    address: Register16::RunLogicControl,
                    value: RunLogicControl::Stop as u16,
                }],
            )),
            self.tqdc_control_addr,
        )
        .await?;
    
        let mut buf = [0; 4096 * 10];
        match time::timeout(Duration::from_millis(100), control.recv_from(&mut buf)).await {
            Ok(res) => match res {
                Ok((_, _)) => Ok(()),
                Err(err) => Err(err.into()),
            },
            Err(_) => Err(Report::msg("(stop_acquisition) - acq timeout")),
        }
    }

    async fn gather_frames(
        self,
        control: Arc<UdpSocket>,
        stream: Arc<UdpSocket>,
    ) -> Result<Vec<[u8; MTU_SIZE]>> {
        let mut frames = Vec::with_capacity(30_000 * 100);
    
        stream
            .send_to(
                &MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
                    0x0000, 0x0101, 0x0001, 0xFFFF, 0xFFFF,
                )),
                self.tqdc_stream_addr,
            )
            .await?;
    
        loop {
            let mut buf = [0; MTU_SIZE];
    
            match tokio::time::timeout(
                Duration::from_millis(500),
                stream.recv_from(&mut buf),
            ).await
            {
                Ok(received) => {
                    let message = MlinkMessage::from_datagram(&buf[..received?.0]);
                    
                    if let MlinkMessage::StreamReq { header, fragment } = message {
                        // let frame = frames.first().unwrap();
                        stream
                        .send_to(
                            // &stream_to_acq_fast(&buf)
                            &MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
                                    header.seq,
                                    0x0001,
                                    0xfefe,
                                    fragment.header.fragment_offset,
                                    fragment.header.fragment_id,
                            ))
                            , 
                            self.tqdc_stream_addr)
                        .await?;
                    }
                    frames.push(buf);
                }
                Err(_) => {
                    let state = self.check_run_state(&control).await?;
    
                    println!("state: {state:?}"); // TODO: remove
                    
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

    pub async fn acquire_point(&self, secs: u32) -> Result<Vec<MStreamADCBlocks>> {

        let control = Arc::new(UdpSocket::bind("0.0.0.0:0").await?); // TODO: remove unwrap
        let stream = Arc::new(UdpSocket::bind("0.0.0.0:0").await?); 

        let millis = secs * 1000;
 
        let gather_loop = {
            let control = control.clone();
            let stream = stream.clone();
            let clone = *self;
            tokio::spawn(TQDC::gather_frames(clone, control, stream))
        };
    
        sleep(Duration::from_millis(100)).await;
        self.start_acquisition(
            millis,
            &control,
        ).await?;
    
        let frames = (gather_loop.await.with_context(|| "gather_loop ")?)?;
    
        self.stop_acquisition(&control).await?;
    
        let events = TQDC::frames_to_events(frames)?;
        Ok(events)
    }


    fn frames_to_events(frames: Vec<[u8; MTU_SIZE]>) -> Result<Vec<MStreamADCBlocks>> {
        let mut fragments = BTreeMap::new();
    
        for frame in frames {
            if let MlinkMessage::StreamReq { fragment, .. } = MlinkMessage::from_datagram(&frame) {
                fragments.entry(fragment.header.fragment_id).or_insert(BTreeMap::new()).insert(
                    fragment.header.fragment_offset, fragment);
            } else {
                Err(Report::msg(
                    "(gather_frames) - incoming packet is not STREAM type",
                ))?
            }
        };
    
        Ok(fragments.values().map(|fragments| {
            
            let merged = MStreamTriggerAndUserData::try_from(
                fragments.values().collect::<Vec<_>>().as_slice()
            ).unwrap(); // TODO: handle error
            let adc_blocks = merged.extract_data_blocks();
    
            MStreamADCBlocks {
                device_serial: merged.device_serial,
                event_number: merged.event_number,
                user_defined_bits: merged.user_defined_bits,
                tai_sec: merged.tai_sec,
                tai_nano_sec: merged.tai_nano_sec,
                adc_blocks,
            }
        }).collect::<Vec<_>>())
    }
}

#[tokio::test]
async fn test() {
    let tqdc = TQDC::new([10, 0, 0, 4]);
    tqdc.acquire_point(10).await.unwrap();
}
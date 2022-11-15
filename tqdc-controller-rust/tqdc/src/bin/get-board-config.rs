use tokio::io;
use tqdc::config::{HOST_IP, CONTROL_HOST_PORT, BOARD_IP, CONTROL_PORT};
use std::net::SocketAddrV4;
use tokio::net::UdpSocket;

use tqdc::regs::{Register16, Register32};
use tqdc::mlink::{MlinkMessage, CtrlReg};

use tokio::io::AsyncReadExt;

use serde_json::json;

const TQDC16VS_ADC_SCALE: u32 = 3;
const TQDC_HPTDC_SCALE: u32 = 1;
const TQDC16VS_ADC_LAT_OFFSET: u32 = 1; // in 8ns steps
const TQDC16VS_HPTDC_LAT_OFFSET: u32 = 5; // in 24ns steps

#[tokio::main]
async fn main() -> io::Result<()> {
    
    Ok(())


    // let val: toml::Value = toml::from_str(&str).unwrap();
    // println!("{val}");

    // let msg = MlinkMessage::new_ctrl_req(
    //     0x0000, 
    //     0x0001, 
    //     0xFEFE, 
    //     vec![
    //         CtrlReg::Read16 { 
    //             address: Register16::DeviceId, 
    //             value: 0x0000 
    //         },
    //         CtrlReg::Read32 {
    //             address: Register32::TimeLimit,
    //             value: 0x0000
    //         },
    //         CtrlReg::Read32 {
    //             address: Register32::RunMode,
    //             value: 0x0000
    //         },
    //         CtrlReg::Read32 {
    //             address: Register32::TriggerTimerPeriod,
    //             value: 0x0000
    //         },
    //         CtrlReg::Read16 {
    //             address: Register16::SelfTriggerDelay,
    //             value: 0x0000
    //         },
    //         CtrlReg::Read16 {
    //             address: Register16::SelfTriggerMask,
    //             value: 0x0000
    //         },
    //         CtrlReg::Read16 {
    //             address: Register16::AdcChannelEnabledMask,
    //             value: 0x0000
    //         },
    //         CtrlReg::Read16 {
    //             address: Register16::AdcGainControlMask,
    //             value: 0x0000
    //         },
    //         CtrlReg::Read16 {
    //             address: Register16::AdcMode,
    //             value: 0x0000
    //         },

    //         // regOp << RegWrite32(REG32_TDC_GEN_CTRL, REG32_TDC_GEN_CTRL_BIT_STANDBY);
    //         // regOpExec(regOp);
    //         // waitHptdcInState(HPTDC_STATE_BIT_INIT);

    //         CtrlReg::Read32 {
    //             address: Register32::TdcChannelEnable1,
    //             value: 0x0000
    //         },

    //         CtrlReg::Read32 {
    //             address: Register32::TdcChannelEnable2,
    //             value: 0x0000
    //         },

    //         CtrlReg::Read32 {
    //             address: Register32::TdcTrigWinSetup,
    //             value: 0x0000
    //         },

    //         CtrlReg::Read16 {
    //             address: Register16::Dac2Ctrl,
    //             value: 0x0000
    //         },
    //     ]
    // );

    // // HOST_IP, CONTROL_HOST_PORT

    // let sock = UdpSocket::bind(format!("{HOST_IP}:{CONTROL_HOST_PORT}")).await?;
    // let to_addr = format!("{BOARD_IP}:{CONTROL_PORT}").parse::<SocketAddrV4>().unwrap();

    // sock.send_to(&MlinkMessage::to_datagram(&msg), to_addr).await?;

    // let mut buf = [0; 4096 * 10];
    // let (len, _) = sock.recv_from(&mut buf).await?;


    // let acq = MlinkMessage::from_datagram(&buf[..len]);

    // println!("{:?}", &acq);

    // let mut match_window = 0;
    // let mut latency = 0;
    // let mut ch_trigger_enabled = [false; 16];
    // let mut ch_enabled = [false; 16];
    // let mut ch_gain = [false; 16];
    // let mut ch_tdc_enable = [false; 16];

    // if let MlinkMessage::CtrlAck { header, regs } = acq {

        
    //     for reg in regs {
    //         match reg {
    //             CtrlReg::Read16 { address, value } => {


    //                 match address {
                        
    //                     Register16::DeviceId => {

    //                     }

    //                     Register16::AdcMode => {

    //                     }

    //                     Register16::SelfTriggerDelay => {

    //                     }

    //                     Register16::SelfTriggerMask => {
    //                         for ch_idx in 0..16 {
    //                             ch_trigger_enabled[ch_idx] = (value >> ch_idx & 1) == 1;
    //                         }
    //                     }
    //                     Register16::AdcChannelEnabledMask => {
    //                         for ch_idx in 0..16 {
    //                             ch_enabled[ch_idx] = (value >> ch_idx & 1) == 1;
    //                         }
    //                     }
    //                     Register16::AdcGainControlMask => {
    //                         for ch_idx in 0..16 {
    //                             ch_gain[ch_idx] = (value >> ch_idx & 1) == 1;
    //                         }
    //                     }
    //                     other => {}
    //                 }

    //             }
    //             CtrlReg::Read32 { address, value } => {
    //                 match address {

    //                     Register32::TimeLimit => {

    //                     }

    //                     Register32::RunMode => {

    //                     }

    //                     Register32::TriggerTimerPeriod => {

    //                     }

    //                     Register32::TdcChannelEnable1 => {
    //                         for idx in 0..8 {
    //                             ch_tdc_enable[15 - idx] = ((value >> (idx * 4)) & 0xF) == 0xF
    //                         }
    //                     }
    //                     Register32::TdcChannelEnable2 => {
    //                         for idx in 0..8 {
    //                             ch_tdc_enable[7 - idx] = ((value >> (idx * 4)) & 0xF) == 0xF
    //                         }
    //                     }
    //                     Register32::TdcTrigWinSetup => {

    //                         match_window = (value >> 12) / TQDC_HPTDC_SCALE;
    //                         latency = ((value & 0xFFF) - 5) / TQDC_HPTDC_SCALE;


    //                         // const int TQDC_CH_TO_DAC_CH_MAP[16] = {
    //                         //     3,2,1,0, 4,5,6,7, 0,1,2,3, 7,6,5,4
    //                         // };
                            
    //                         // quint32 mW = tqdc_hptdc_scale*setup.matchWin;
    //                         // quint32 lat = tqdc16vs_hptdc_lat_offset + tqdc_hptdc_scale*setup.latency;
    //                         // regOp << RegWrite32(REG32_TDC_TRIG_WIN_SETUP,
    //                         //                     (mW&0xFFF)<<12 | (lat & 0xFFF));


    //                         // regOp << RegWrite32(REG32_TDC_TRIG_WIN_SETUP,
    //                         //     (mW&0xFFF)<<12 | (lat & 0xFFF));
    //                     }
    //                     _ => panic!()
    //                 }
    //             }
    //             _ => todo!()
    //         }
    //     }
    // }

    // println!("{match_window} {latency}");

    // println!("{ch_enabled:?}");
    // println!("{ch_trigger_enabled:?}");
    // println!("{ch_gain:?}");
    // println!("{ch_tdc_enable:?}");

    // Ok(())
}
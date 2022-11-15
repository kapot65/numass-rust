use std::convert::TryFrom;
use ux::*;

use crate::regs::{Register32, Register16, is_reg16, is_reg32};

#[derive(Debug)]
pub enum MlinkMessage {
    StreamReq {
        header: MLinkHeader, 
        frames: Vec<MStreamFragment>
    },
    StreamAcq {
        header: MLinkHeader,
        offset: u16,
        id: u16,
    },
    CtrlReq {
        header: MLinkHeader,
        regs: Vec<CtrlReg>
    },
    CtrlAck {
        header: MLinkHeader,
        regs: Vec<CtrlReg>
    }
}

#[derive(Debug)]
pub enum CtrlReg {
    Read16 {
        address: Register16,
        value: u16
    },
    Write16 {
        address: Register16,
        value: u16
    },
    Read32 {
        address: Register32,
        value: u32
    },
    Write32 {
        address: Register32,
        value: u32
    }
}


impl MlinkMessage {

    pub fn new_stream_acq(seq: u16, src: u16, dst: u16, offset: u16, id: u16) -> MlinkMessage {
        MlinkMessage::StreamAcq { header: MLinkHeader {
            type_ : MessageType::Stream,
            sync: MessageSync::MlFrameSync,
            seq,
            len: 5,
            src,
            dst,
        }, offset, id }  
    }

    pub fn new_ctrl_req(seq: u16, src: u16, dst: u16, regs: Vec<CtrlReg>) -> MlinkMessage {

        let size =  MlinkMessage::calculate_regs_len(&regs);
        MlinkMessage::CtrlReq { header: MLinkHeader {
                type_ : MessageType::CtrlReq,
                sync: MessageSync::MlFrameSync,
                seq,
                len: 3 + size + 1,
                src,
                dst,
            }, regs
        }
    }

    pub fn new_ctrl_acq(seq: u16, src: u16, dst: u16, regs: Vec<CtrlReg>) -> MlinkMessage {
        
        let size =  MlinkMessage::calculate_regs_len(&regs);
        MlinkMessage::CtrlAck { header: MLinkHeader {
                type_ : MessageType::CtrlAck,
                sync: MessageSync::MlFrameSync,
                seq,
                len: 3 + size + 1,
                src,
                dst,
            }, regs
        }
    }

    pub fn from_datagram(data: &[u8]) -> MlinkMessage {
        // TODO: remove duplicate check?
        assert!(data.len() >= 12, "packet length less than header length (12 bytes)");
        let header = MLinkHeader::from_bytes(&data[..12]);

        match header.type_ {
            MessageType::Stream => {
                if header.len == 5 {
                    MlinkMessage::StreamAcq { 
                        header, 
                        offset: u16::from_le_bytes(data[16..18].try_into().unwrap()), 
                        id: u16::from_le_bytes(data[18..20].try_into().unwrap())
                    }
                } else {
                    let mut offset = 12;
                    let mut frames = vec![];

                    while offset < data.len() - 4 {
                        let frame = MStreamFragment::from_bytes(&data[offset..]);
                        offset += (frame.length + 8) as usize;
                        frames.push(frame);
                    }

                    MlinkMessage::StreamReq { header, frames }
                }
            }

            MessageType::CtrlAck => {
                let regs = MlinkMessage::extract_regs(&data);
                MlinkMessage::CtrlAck { header, regs } 
            }

            MessageType::CtrlReq => {
                let regs = MlinkMessage::extract_regs(&data);
                MlinkMessage::CtrlReq { header, regs } 
            },
        }
    }  

    pub fn to_datagram(message: &MlinkMessage) -> Vec<u8> {

        return match message {
            MlinkMessage::StreamAcq {header, offset, id} => {
                
                let mut buffer = Vec::with_capacity(12 + 8 + 4);
                
                MlinkMessage::write_header(header, &mut buffer);

                let word1 = (1 << 24 | 1 << 22) as u32;
                buffer.extend_from_slice(&word1.to_le_bytes());

                let word2 = (*id as u32) << 16 | (*offset as u32);
                buffer.extend_from_slice(&word2.to_le_bytes());

                MlinkMessage::write_crc(&mut buffer);

                buffer
            },

            MlinkMessage::StreamReq { header: _, frames: _ } => {
                panic!("serialization is not defined for MlinkMessage::StreamReq");
            }

            MlinkMessage::CtrlReq { header, regs } => {

                let mut buffer = Vec::with_capacity((header.len * 4).into());
                
                MlinkMessage::write_header(header, &mut buffer);
                MlinkMessage::write_regs(regs, &mut buffer);
                MlinkMessage::write_crc(&mut buffer);

                buffer
            }

            MlinkMessage::CtrlAck { header, regs } => {

                let mut buffer = Vec::with_capacity((header.len * 4).into());
                
                MlinkMessage::write_header(header, &mut buffer);
                MlinkMessage::write_regs(regs, &mut buffer);
                MlinkMessage::write_crc(&mut buffer);

                buffer
            }
        }
    }

    fn read_word(data: &[u8], offset: usize) -> (bool, u16, u16) {
        let word = u32::from_le_bytes(data[offset..offset+4].try_into().unwrap());

        ((word >> 31) == 1, ((word >> 16) & 0x7FFF) as u16, (word & 0xFFFF) as u16)
        
    }

    fn calculate_regs_len(regs: &Vec<CtrlReg>) -> u16 {
        regs.iter().map(|r| {
            match r {
                CtrlReg::Read16 { address: _, value: _ } => 1,
                CtrlReg::Write16 { address: _, value: _ } => 1,
                CtrlReg::Read32 { address: _, value: _ } => 2,
                CtrlReg::Write32 { address: _, value: _ } => 2,
            }
        }).reduce(|s1, s2| s1 + s2).unwrap()
    }

    fn extract_regs(data: &[u8]) -> Vec<CtrlReg> {
        let mut offset = 12;

        let mut regs = vec![];
        while offset < data.len() - 4 {

            let (r1, a1, v1) = MlinkMessage::read_word(data, offset);
            offset += 4;

            if let Some(address) = is_reg32(a1){
                assert!(data.len() - 8 >= offset);
                
                let (r2, a2, v2) = MlinkMessage::read_word(data, offset);
                offset += 4;

                assert!(r1 == r2 && a1 == a2 - 1);
                if r1 {
                    regs.push(CtrlReg::Read32 { address, value: (v2 as u32) << 16 | (v1 as u32) })
                } else {
                    regs.push(CtrlReg::Write32 { address, value: (v2 as u32) << 16 | (v1 as u32) })
                }
            } else if let Some(address) = is_reg16(a1) {
                if r1 {
                    regs.push(CtrlReg::Read16 { address, value: v1 })
                } else {
                    regs.push(CtrlReg::Write16 { address, value: v1 })
                }
            } else {
                panic!("unknown register 0x{}", a1)
            }
        }

        regs
    }

    fn write_header(header: &MLinkHeader, buffer: &mut Vec<u8>) {
        let type_bin = match header.type_ {
            MessageType::Stream => (MessageType::Stream as u16).to_le_bytes(),
            MessageType::CtrlAck => (MessageType::CtrlAck as u16).to_le_bytes(),
            MessageType::CtrlReq => (MessageType::CtrlReq as u16).to_le_bytes(),
        };
        buffer.extend_from_slice(&type_bin);

        let sync_bin = match header.sync {
            MessageSync::MlFrameSync => (MessageSync::MlFrameSync as u16).to_le_bytes()
        };
        buffer.extend_from_slice(&sync_bin);

        buffer.extend_from_slice(&header.seq.to_le_bytes());
        buffer.extend_from_slice(&header.len.to_le_bytes());
        buffer.extend_from_slice(&header.src.to_le_bytes());
        buffer.extend_from_slice(&header.dst.to_le_bytes());
    }

    fn write_regs(regs: &Vec<CtrlReg>, buffer: &mut Vec<u8>) {
        for reg in regs {
            match reg {
                CtrlReg::Read16 { address, value } => {
                    let word: u32 = 0x80000000 | ((*address as u32 & 0x7FFF) << 16) | (*value as u32);
                    buffer.extend_from_slice(&word.to_le_bytes());
                }

                CtrlReg::Write16 { address, value} => {
                    let word: u32 = 0x00000000 | ((*address as u32 & 0x7FFF) << 16) | (*value as u32);
                    buffer.extend_from_slice(&word.to_le_bytes());
                }

                CtrlReg::Read32 { address, value } => {
                    let word1: u32 = 0x80000000 | ((*address as u32 & 0x7FFF) << 16) | (*value & 0xFFFF);
                    let word2: u32 = 0x80000000 | (((*address as u32 + 1) as u32 & 0x7FFF) << 16) | (*value >> 16);
                    buffer.extend_from_slice(&word1.to_le_bytes());
                    buffer.extend_from_slice(&word2.to_le_bytes());
                }

                CtrlReg::Write32 { address, value} => {
                    let word1: u32 = 0x00000000 | ((*address as u32 & 0x7FFF) << 16) | (*value & 0xFFFF);
                    let word2: u32 = 0x00000000 | (((*address as u32 + 1) as u32 & 0x7FFF) << 16) | (*value >> 16);
                    buffer.extend_from_slice(&word1.to_le_bytes());
                    buffer.extend_from_slice(&word2.to_le_bytes());
                }
            }
        }
    }

    fn write_crc(buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&vec![0,0,0,0]);
    }  
}

#[derive(Debug)]
pub enum MessageType {
    Stream = 0x5354,
    CtrlReq = 0x0101,
    CtrlAck = 0x0102,
}

impl TryFrom<u16> for MessageType {
    type Error = ();

    fn try_from(v: u16) -> Result<Self, Self::Error> {
        match v {
            code if code == MessageType::Stream as u16 => Ok(MessageType::Stream),
            code if code == MessageType::CtrlAck as u16 => Ok(MessageType::CtrlAck),
            code if code == MessageType::CtrlReq as u16 => Ok(MessageType::CtrlReq),
            code => panic!("No message type for 0x{:x} code!", code),
        }
    }
}

#[derive(Debug)]
pub enum MessageSync {
    MlFrameSync = 0x2A50,
}

impl TryFrom<u16> for MessageSync {
    type Error = ();

    fn try_from(v: u16) -> Result<Self, Self::Error> {
        match v {
            code if code == MessageSync::MlFrameSync as u16 => Ok(MessageSync::MlFrameSync),
            code => panic!("No message sync for 0X{:x} code!", code),
        }
    }
}


#[derive(Debug)]
pub struct MLinkHeader { // TODO: remove type, sync and mb seq
    type_: MessageType,
    sync: MessageSync,
    pub seq: u16,
    len: u16,
    pub src: u16,
    pub dst: u16,
}

impl MLinkHeader {
    fn from_bytes(bytes: &[u8]) -> MLinkHeader {
        assert!(bytes.len() >= 12); // TODO: remove duplicate check?

        let type_ = MessageType::try_from(u16::from_le_bytes(bytes[..2].try_into().unwrap())).unwrap();
        let sync = MessageSync::try_from(u16::from_le_bytes(bytes[2..4].try_into().unwrap())).unwrap();

        return MLinkHeader {
            type_, sync,
            seq: u16::from_le_bytes(bytes[4..6].try_into().unwrap()),
            len: u16::from_le_bytes(bytes[6..8].try_into().unwrap()),
            src: u16::from_le_bytes(bytes[8..10].try_into().unwrap()),
            dst: u16::from_le_bytes(bytes[10..12].try_into().unwrap()),
        };
    }
}

#[derive(Debug)]
pub struct ADCDataBlock {
    pub channel_number: u8,
    pub waveform: Vec<i16>
}

#[derive(Debug)]
pub struct MStreamFragment {
    pub length: u16, // ! actual frame length is = lenght + 8
    pub subtype_and_flags: u8,
    pub device_id: u8,
    pub fragment_offset: u16,
    pub fragment_id: u16,
    pub device_serial: u32,
    pub event_number: u24,
    pub user_defined_bits: u8,
    pub tai_sec: u32,
    pub tai_nano_sec: u32,
    pub channels: Vec<ADCDataBlock>
}

impl MStreamFragment {
    fn from_bytes(bytes: &[u8]) -> MStreamFragment {
        assert!(
            bytes.len() >= 32, 
            "buffer size ({}) < 32 bytes needed for header", 
            bytes.len()
        );

        // TODO: add bytes len checking
        // M-Stream Header
        // Word #1 
        let length = u16::from_le_bytes(bytes[..2].try_into().unwrap()); // 15:0
        let subtype_and_flags = u8::from_le_bytes(bytes[2..3].try_into().unwrap()); // 23:16
        // TODO add subtype and flags parsing
        assert!(subtype_and_flags == 128, "unexpected subtype and flags byte: {subtype_and_flags}");
        let device_id = u8::from_le_bytes(bytes[3..4].try_into().unwrap()); // 31:24

        // Word #2
        let fragment_offset = u16::from_le_bytes(bytes[4..6].try_into().unwrap()); // 15:0
        assert!(fragment_offset == 0, "splitted packets (offset = {fragment_offset}) is not supported");
        let fragment_id = u16::from_le_bytes(bytes[6..8].try_into().unwrap()); //31:16

        // Word #3
        let device_serial = u32::from_le_bytes(bytes[8..12].try_into().unwrap()); // 31:0

        // Word #4
        let event_number = u24::new(u32::from_le_bytes(bytes[12..16].try_into().unwrap()) & 0x00FFFFFF);
        let user_defined_bits = u8::from_le_bytes(bytes[15..16].try_into().unwrap());

        let tai_sec = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let tai_nano_sec = u32::from_le_bytes(bytes[20..24].try_into().unwrap()); // ! 1:0 - flag


        // M-Stream Subtype 0
        let mut offset = 24;
        let mut channels = vec![];

        while offset < (length + 8) as usize {
            let data_payload_length = u16::from_le_bytes(bytes[offset..offset+2].try_into().unwrap()); // 15:0
            // let adc_data_block_specific = {
            //     let combined = u8::from_le_bytes(bytes[offset+2..offset+3].try_into().unwrap()); // 23:16
            //     combined & 0b11 // 16..18
            // };
            
            let (data_type, channel_number) = {
                let combined = u8::from_le_bytes(bytes[offset+3..offset+4].try_into().unwrap());
                (combined >> 4, combined & 0b1111)
            };

            match data_type {
                0 => {
                    // TODO add TDC event parsing
                }
                1 => {
                    // let adc_timestamp = u16::from_le_bytes(bytes[offset+4..offset+6].try_into().unwrap());
                    let adc_data_length = u16::from_le_bytes(bytes[offset+6..offset+8].try_into().unwrap());


                    let bins_number = (adc_data_length / 2) as usize;
                    let mut waveform = Vec::with_capacity(bins_number);
                    for n in 0..bins_number {
                        waveform.push(i16::from_le_bytes(
                            bytes[offset + 8 + (n * 2)..offset + 8 + ((n+1) * 2)]
                            .try_into().unwrap()));
                    }
                    channels.push(ADCDataBlock { channel_number, waveform });
                }
                other => panic!("unexpected MStream Data Block format {other}")
            }
            offset += data_payload_length as usize + 4;
        }

        MStreamFragment {
            length,
            subtype_and_flags,
            device_id,
            fragment_offset,
            fragment_id,
            device_serial,
            event_number,
            user_defined_bits,
            tai_sec,
            tai_nano_sec,
            channels
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use tokio::io::{AsyncReadExt};

    #[tokio::test]
    async fn parse_mstream_data_samples() -> tokio::io::Result<()> {

        let mut file = tokio::fs::File::open(
            // "./test-data/frames/0l.bin"
            // "./test-data/frames/subs-overflow.bin"
            // "./test-data/frames/trash-3.bin"
            "../test-data/frames/0l-cropped-2.bin"
            // "./test-data/frames/408ns-7ch.bin"
        ).await?;
    
        let mut contents = [0u8; 2048];
        let size = file.read(&mut contents).await?;
    
        let message = MlinkMessage::from_datagram(&contents[..size]);
        
        println!("{message:?}");

        Ok(())
    }

    #[test]
    fn parse_stream_req() {
        
        let offset = 0x0001;
        let id = 0x0002;

        let packet = MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
            0x0000, 
            0x0101, 
            0x0001, 
            offset, id
        ));

        let parsed = MlinkMessage::from_datagram(&packet);
        println!("{:?}", parsed)
    }
}

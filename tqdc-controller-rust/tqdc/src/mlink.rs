use std::convert::TryFrom;

use arrayref::array_ref;

use crate::regs::{is_reg16, is_reg32, Register16, Register32};
use crate::mstream::MStreamFragment;

#[derive(Debug)]
pub enum MlinkMessage {
    StreamReq {
        header: MLinkHeader,
        fragment: MStreamFragment,
    },
    StreamAcq {
        header: MLinkHeader,
        offset: u16,
        id: u16,
    },
    CtrlReq {
        header: MLinkHeader,
        regs: Vec<CtrlReg>,
    },
    CtrlAck {
        header: MLinkHeader,
        regs: Vec<CtrlReg>,
    },
}

#[derive(Debug)]
pub enum CtrlReg {
    Read16 { address: Register16, value: u16 },
    Write16 { address: Register16, value: u16 },
    Read32 { address: Register32, value: u32 },
    Write32 { address: Register32, value: u32 },
}

impl MlinkMessage {
    pub fn new_stream_acq(seq: u16, src: u16, dst: u16, offset: u16, id: u16) -> MlinkMessage {
        MlinkMessage::StreamAcq {
            header: MLinkHeader {
                type_: MessageType::Stream,
                sync: MessageSync::MlFrameSync,
                seq,
                len: 6,
                src,
                dst,
            },
            offset,
            id,
        }
    }

    pub fn new_ctrl_req(seq: u16, src: u16, dst: u16, regs: Vec<CtrlReg>) -> MlinkMessage {
        let size = MlinkMessage::calculate_regs_len(&regs);
        MlinkMessage::CtrlReq {
            header: MLinkHeader {
                type_: MessageType::CtrlReq,
                sync: MessageSync::MlFrameSync,
                seq,
                len: 3 + size + 1,
                src,
                dst,
            },
            regs,
        }
    }

    pub fn new_ctrl_acq(seq: u16, src: u16, dst: u16, regs: Vec<CtrlReg>) -> MlinkMessage {
        let size = MlinkMessage::calculate_regs_len(&regs);
        MlinkMessage::CtrlAck {
            header: MLinkHeader {
                type_: MessageType::CtrlAck,
                sync: MessageSync::MlFrameSync,
                seq,
                len: 3 + size + 1,
                src,
                dst,
            },
            regs,
        }
    }

    pub fn from_datagram(data: &[u8]) -> MlinkMessage {
        // TODO: remove duplicate check?
        assert!(
            data.len() >= 12,
            "packet length less than header length (12 bytes)"
        );
        let header = MLinkHeader::from_bytes(&data[..12]);
        let data = &data[12..(header.len as usize) * 4];

        match header.type_ {
            MessageType::Stream => {

                let fragment = MStreamFragment::from(data);
                MlinkMessage::StreamReq { header, fragment }
            }

            MessageType::CtrlAck => {
                let regs = MlinkMessage::extract_regs(data);
                MlinkMessage::CtrlAck { header, regs }
            }

            MessageType::CtrlReq => {
                let regs = MlinkMessage::extract_regs(data);
                MlinkMessage::CtrlReq { header, regs }
            }
        }
    }

    pub fn to_datagram(message: &MlinkMessage) -> Vec<u8> {
        match message {
            MlinkMessage::StreamAcq { header, offset, id } => {
                let mut buffer = Vec::with_capacity(12 + 8 + 4);

                MlinkMessage::write_header(header, &mut buffer);

                let word1 = (1 << 24 | 1 << 18) as u32;
                buffer.extend_from_slice(&word1.to_le_bytes());

                let word2 = (*id as u32) << 16 | (*offset as u32);
                buffer.extend_from_slice(&word2.to_le_bytes());

                MlinkMessage::write_crc(&mut buffer);

                buffer
            }

            MlinkMessage::StreamReq { .. } => {
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
        let word = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        (
            (word >> 31) == 1,
            ((word >> 16) & 0x7FFF) as u16,
            (word & 0xFFFF) as u16,
        )
    }

    fn calculate_regs_len(regs: &[CtrlReg]) -> u16 {
        regs.iter()
            .map(|r| match r {
                CtrlReg::Read16 { .. } => 1,
                CtrlReg::Write16 { .. } => 1,
                CtrlReg::Read32 { .. } => 2,
                CtrlReg::Write32 { .. } => 2,
            })
            .reduce(|s1, s2| s1 + s2)
            .unwrap()
    }

    /// ! data should not contain header and crc
    fn extract_regs(data: &[u8]) -> Vec<CtrlReg> {
        let mut offset = 0;

        let mut regs = vec![];
        while offset < data.len() - 4 {
            let (r1, a1, v1) = MlinkMessage::read_word(data, offset);
            offset += 4;

            if let Some(address) = is_reg32(a1) {
                assert!(data.len() - 8 >= offset);

                let (r2, a2, v2) = MlinkMessage::read_word(data, offset);
                offset += 4;

                assert!(r1 == r2 && a1 == a2 - 1);
                if r1 {
                    regs.push(CtrlReg::Read32 {
                        address,
                        value: (v2 as u32) << 16 | (v1 as u32),
                    })
                } else {
                    regs.push(CtrlReg::Write32 {
                        address,
                        value: (v2 as u32) << 16 | (v1 as u32),
                    })
                }
            } else if let Some(address) = is_reg16(a1) {
                if r1 {
                    regs.push(CtrlReg::Read16 { address, value: v1 })
                } else {
                    regs.push(CtrlReg::Write16 { address, value: v1 })
                }
            } else {
                panic!("unknown register 0x{a1}")
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
            MessageSync::MlFrameSync => (MessageSync::MlFrameSync as u16).to_le_bytes(),
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
                    let word: u32 =
                        0x80000000 | ((*address as u32 & 0x7FFF) << 16) | (*value as u32);
                    buffer.extend_from_slice(&word.to_le_bytes());
                }

                CtrlReg::Write16 { address, value } => {
                    let word: u32 = ((*address as u32 & 0x7FFF) << 16) | (*value as u32);
                    buffer.extend_from_slice(&word.to_le_bytes());
                }

                CtrlReg::Read32 { address, value } => {
                    let word1: u32 =
                        0x80000000 | ((*address as u32 & 0x7FFF) << 16) | (*value & 0xFFFF);
                    let word2: u32 =
                        0x80000000 | (((*address as u32 + 1) & 0x7FFF) << 16) | (*value >> 16);
                    buffer.extend_from_slice(&word1.to_le_bytes());
                    buffer.extend_from_slice(&word2.to_le_bytes());
                }

                CtrlReg::Write32 { address, value } => {
                    let word1: u32 = ((*address as u32 & 0x7FFF) << 16) | (*value & 0xFFFF);
                    let word2: u32 = (((*address as u32 + 1) & 0x7FFF) << 16) | (*value >> 16);
                    buffer.extend_from_slice(&word1.to_le_bytes());
                    buffer.extend_from_slice(&word2.to_le_bytes());
                }
            }
        }
    }

    fn write_crc(buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(&[0x49, 0x62, 0x20, 0x12]);
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
            code => panic!("No message type for 0x{code:x} code!"),
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
            code => panic!("No message sync for 0X{code:x} code!"),
        }
    }
}

#[derive(Debug)]
pub struct MLinkHeader {
    // TODO: remove type, sync and mb seq
    type_: MessageType,
    sync: MessageSync,
    pub seq: u16,
    len: u16,
    pub src: u16,
    pub dst: u16,
}

impl MLinkHeader {
    pub fn from_bytes(bytes: &[u8]) -> MLinkHeader {
        assert!(bytes.len() >= 12); // TODO: remove duplicate check?

        let type_ =
            MessageType::try_from(u16::from_le_bytes(*array_ref![bytes, 0, 2])).unwrap();
        let sync =
            MessageSync::try_from(u16::from_le_bytes(*array_ref![bytes, 2, 2])).unwrap();

        MLinkHeader {
            type_,
            sync,
            seq: u16::from_le_bytes(*array_ref![bytes, 4, 2]),
            len: u16::from_le_bytes(*array_ref![bytes, 6, 2]),
            src: u16::from_le_bytes(*array_ref![bytes, 8, 2]),
            dst: u16::from_le_bytes(*array_ref![bytes, 10, 2]),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn parse_mstream_data_samples() -> tokio::io::Result<()> {
        let mut file = tokio::fs::File::open(
            // "../resources/test/frames/0l.bin"
            // "./resources/test/frames/subs-overflow.bin"
            // "./resources/test/frames/trash-3.bin"
            "../resources/test/frames/0l-cropped-2.bin", // "./resources/test/frames/408ns-7ch.bin"
        )
        .await?;

        let meta = file.metadata().await.unwrap();

        let mut contents = [0u8; 2048];
        let size = file.read(&mut contents).await?;

        assert!(size == meta.len() as usize);

        let message = MlinkMessage::from_datagram(&contents[..]);

        println!("{message:?}");

        Ok(())
    }

    #[test]
    fn parse_stream_req() {
        let offset = 0x0001;
        let id = 0x0002;

        let packet = MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
            0x0000, 0x0101, 0x0001, offset, id,
        ));

        let parsed = MlinkMessage::from_datagram(&packet);
        println!("{parsed:?}")
    }
}

use std::collections::BTreeMap;

use arrayref::array_ref;
use log::debug;
use ux::u24;

#[derive(Debug)] // TODO: merge with RawWaveform
pub struct ADCDataBlock {
    pub ch_num: u8,
    pub waveform: Vec<i16>,
}

#[derive(Debug)]
pub struct MStreamFragment {
    pub header: MStreamHeader,
    pub payload: Vec<u8>,
}

#[derive(Debug)]
pub struct MStreamHeader {
    pub device_id: u8,
    pub subtype_and_flags: MStreamFragmentFlags,
    pub length: u16, // ! actual frame length is = lenght + 8 ? (is fixed?)
    pub fragment_offset: u16,
    pub fragment_id: u16,
}

#[derive(Debug)]
pub struct MStreamFragmentFlags(u8);
#[derive(Debug)]
pub enum MStreamSubtype {
    TriggerAndUserData,
    ChannelBasedReadout,
    TimeSliceBasedReadout,
}

impl From<&[u8; 8]> for MStreamHeader {
    fn from(bytes: &[u8; 8]) -> Self {
        Self {
            // Word #1
            length: u16::from_le_bytes([bytes[0], bytes[1]]), // 15:0,
            subtype_and_flags: bytes[2].into(), // 23:16 
            device_id: bytes[3], // 31:24

            // Word #2
            fragment_offset: u16::from_le_bytes([bytes[4], bytes[5]]), // 15:0
            fragment_id: u16::from_le_bytes([bytes[6], bytes[7]]) // 31:16
        }
    }
}

impl MStreamFragmentFlags {

    pub fn subtype(&self) -> MStreamSubtype {
        match self.0 & 0b11 {
            0 => MStreamSubtype::TriggerAndUserData,
            1 => MStreamSubtype::ChannelBasedReadout,
            2 => MStreamSubtype::TimeSliceBasedReadout,
            other => panic!("unexpected MStream Subtype {other}")
        }
    }

    pub fn acq(&self) -> bool { self.0 >> 2 & 0b1 == 1 }
    pub fn rst(&self) -> bool { self.0 >> 3 & 0b1 == 1 }
    pub fn syn(&self) -> bool { self.0 >> 4 & 0b1 == 1 }
    pub fn fin(&self) -> bool { self.0 >> 5 & 0b1 == 1 }
    pub fn evc(&self) -> bool { self.0 >> 6 & 0b1 == 1 }
    pub fn last_fragment(&self) -> bool { self.0 >> 7 & 0b1 == 1 }
}

impl From<u8> for MStreamFragmentFlags {
    fn from(v: u8) -> Self {
        Self(v)
    }
}

impl From<&[u8]> for MStreamFragment {
    fn from(bytes: &[u8]) -> Self {
        let header = MStreamHeader::from(array_ref!(bytes, 0, 8));
        let length = header.length as usize;
        Self {
            header,
            payload: bytes[8..8 + length].to_vec()
        }
    }
}

#[derive(Debug)]
pub struct MStreamTriggerAndUserData {
    pub device_serial: u32,
    pub event_number: u24,
    pub user_defined_bits: u8,
    pub tai_sec: u32,
    pub tai_nano_sec: u32,
    pub user_data: Vec<u8>,
}

#[derive(Debug)]
pub struct MStreamADCBlocks {
    pub device_serial: u32,
    pub event_number: u24,
    pub user_defined_bits: u8,
    pub tai_sec: u32,
    pub tai_nano_sec: u32,
    pub adc_blocks: Vec<ADCDataBlock>,
}

impl TryFrom<&[&MStreamFragment]> for MStreamTriggerAndUserData {

    type Error = &'static str;

    fn try_from(fragments: &[&MStreamFragment]) -> Result<Self, Self::Error> {

        let mut frames = BTreeMap::new();

        fragments.iter().for_each(|fragment| {
            let group = frames.entry(fragment.header.fragment_id)
                .or_insert(BTreeMap::new());
            group.insert(fragment.header.fragment_offset, fragment);
        });

        // check if all fragments are present and in order
        let mut total_length = 0;
        {
            let mut last_fragment_found = false;
            let mut current_offset = 0;
            
            for fragment in fragments {
                if fragment.header.fragment_offset != current_offset {
                    fragments.iter().for_each(|fragment| debug!("{:?}", fragment.header));
                    Err("missing intermediate fragment")?;
                }
                if fragment.header.subtype_and_flags.last_fragment() {
                    last_fragment_found = true;
                }
                current_offset += fragment.header.length;
                total_length += fragment.header.length as usize;
            }

            if !last_fragment_found {
                Err("last fragment not found")?;
            }
        }

        let mut buffer = Vec::with_capacity(total_length);

        let mut device_serial = None;
        let mut event_number = None;
        let mut user_defined_bits = None;
        let mut tai_sec = None;
        let mut tai_nano_sec = None;

        fragments.iter().for_each(|fragment| {
            let length = fragment.header.length as usize;
            
            if fragment.header.fragment_offset == 0 {
                device_serial = Some(u32::from_le_bytes(*array_ref![fragment.payload, 0, 4]));
                event_number = Some(
                    u24::new(u32::from_le_bytes(*array_ref![fragment.payload, 4, 4]) & 0x00FFFFFF)
                );
                user_defined_bits = Some(fragment.payload[7]);
                tai_sec = Some(u32::from_le_bytes(*array_ref![fragment.payload, 8, 4]));
                tai_nano_sec = Some(u32::from_le_bytes(*array_ref![fragment.payload, 12, 4])); // ! 1:0 - flag

                buffer.extend_from_slice(&fragment.payload[16..length]);
            } else {
                buffer.extend_from_slice(&fragment.payload[..length]); 
            }
        });

        Ok(Self {
            device_serial: device_serial.unwrap(),
            event_number: event_number.unwrap(),
            user_defined_bits: user_defined_bits.unwrap(),
            tai_sec: tai_sec.unwrap(),
            tai_nano_sec: tai_nano_sec.unwrap(),
            user_data: buffer
        })
    }
}

impl MStreamTriggerAndUserData {
    pub fn extract_data_blocks(&self) -> Vec<ADCDataBlock> {

        let mut offset = 0;
        let mut channels = vec![];
    
        while offset < self.user_data.len() {
            let data_payload_length = u16::from_le_bytes(*array_ref![self.user_data, offset, 2]); // 15:0
    
            // let adc_data_block_specific = {
            //     let combined = u8::from_le_bytes(bytes[offset+2..offset+3].try_into().unwrap()); // 23:16
            //     combined & 0b11 // 16..18
            // };
    
            let (data_type, channel_number) = {
                let combined = self.user_data[offset + 3];
                (combined >> 4, combined & 0b1111)
            };
    
            match data_type {
                0 => {
                    // TODO add TDC event parsing
                }
                1 => {
                    // let adc_timestamp = u16::from_le_bytes(bytes[offset+4..offset+6].try_into().unwrap());
                    let adc_data_length =
                        u16::from_le_bytes(*array_ref![self.user_data, offset + 6, 2]);
    
                    let bins_number = (adc_data_length / 2) as usize;
                    let mut waveform = Vec::with_capacity(bins_number);
                    for n in 0..bins_number {
                        waveform.push(i16::from_le_bytes(*array_ref![self.user_data, offset + 8 + (n * 2), 2]));
                    }
    
                    channels.push(ADCDataBlock {
                        ch_num: channel_number,
                        waveform,
                    });
                }
                other => panic!("unexpected MStream Data Block format {other}"),
            }
            offset += data_payload_length as usize + 4;
        }
        channels
    }
}


#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_multifragment() {
        let payload = {
            let fragment_1 = {
                let data = std::fs::read("../resources/test/frames/multi-1.bin").unwrap();
                MStreamFragment::from(&data[12..data.len() - 4])
            };
            let fragment_2 = {
                let data = std::fs::read("../resources/test/frames/multi-2.bin").unwrap();
                MStreamFragment::from(&data[12..data.len() - 4])
            };
            MStreamTriggerAndUserData::try_from([&fragment_1, &fragment_2].as_slice()).unwrap()
        };

        let blocks = payload.extract_data_blocks();
        println!("{blocks:?}");
    }

    #[test]
    fn test_multifragment_wrong_order() {
        let payload = {
            let fragment_1 = {
                let data = std::fs::read("../resources/test/frames/multi-2.bin").unwrap();
                MStreamFragment::from(&data[12..data.len() - 4])
            };
            let fragment_2 = {
                let data = std::fs::read("../resources/test/frames/multi-1.bin").unwrap();
                MStreamFragment::from(&data[12..data.len() - 4])
            };
            MStreamTriggerAndUserData::try_from([&fragment_1, &fragment_2].as_slice()).unwrap()
        };

        let blocks = payload.extract_data_blocks();
        println!("{blocks:?}");
    }
}


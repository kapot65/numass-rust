
pub mod protos;

use std::time::UNIX_EPOCH;
use std::{fmt::Debug, time::SystemTime};
use std::vec;

use serde::{Serialize, Deserialize};
use time::{OffsetDateTime, UtcOffset};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Serialize, Deserialize)]
pub enum ErrorType // TODO rename
{
    ClientNoError = 0,
    UnknownError = 1,
    ServerInitError = 2,
    ClientDisconnect = 3, 
    ParseMessageError = 4, 
    TimeoutError = 5,
    AlgoritmError = 6, 
    UnknownMessageError = 7, 
    ServerBusyError = 8, 
    IncorrectMessageParams = 9, 
    MultipleConnection = 10,
    ComPortError  = 11, 
    ComPortClose = 12,
    Agilent34401aError = 13
}

#[derive(Debug)]
pub struct DFMessage {
    pub meta: DFMeta,
    pub data: Option<Vec<u8>>
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DFMeta {
    #[serde(rename="command")]
    Command(Command),
    #[serde(rename="reply")]
    Reply(Reply)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "command_type")]
pub enum Command {
    #[serde(rename="init")]
    Init,
    #[serde(rename="acquire_point")]
    AcquirePoint {
        split: bool,
        acquisition_time: f32,
        external_meta: Option<serde_json::Value>
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ReplyStatus {
    #[serde(rename="ok")]
    Ok
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "reply_type")]
pub enum Reply {

    #[serde(rename="error")]
    Error {
        error_code: ErrorType,
        description: String
    },
    #[serde(rename="init")]
    Init {
        status: ReplyStatus,
        reseted: bool
    },
    #[serde(rename="aquired_point")]
    AcquirePoint {
        acquisition_time: f32,
        #[serde(with = "time::serde::iso8601")]
        start_time: OffsetDateTime,
        #[serde(with = "time::serde::iso8601")]
        end_time: OffsetDateTime,
        external_meta: Option<serde_json::Value>,
        // split: bool,
        status: ReplyStatus,
    }
}

#[derive(Debug)]
enum MetaType {
    Undefined = 0x00000000,
    Json = 0x00010000,
    Qdatastream = 0x00010007,
}

impl TryFrom<u32> for MetaType {
    type Error = ();
    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v {
            code if code == MetaType::Undefined as u32 => Ok(MetaType::Undefined),
            code if code == MetaType::Json as u32 => Ok(MetaType::Json),
            code if code == MetaType::Qdatastream as u32 => Ok(MetaType::Qdatastream),
            code => panic!("No meta type for 0x{:x} code!", code),
        }
    }
}

const DF01_OPEN_SCOPE: [u8; 2] = [35, 33]; // = #!
const DF01_CLOSE_SCOPE: [u8; 4] = [33, 35, 13, 10]; // = !#\r\n

// TODO extract general DF Envelope parsing from Detector-Specific logic
// TODO handle errors
pub async fn extract_df_message(stream: & mut (impl AsyncReadExt + std::marker::Unpin)) -> DFMessage {

    let header_open_scope = {
        let mut header_open_scope = [0, 0];
        stream.read_exact(&mut header_open_scope).await.unwrap();
        header_open_scope
    };
    
    if header_open_scope == DF01_OPEN_SCOPE {
        
        let header_type = stream.read_u32().await.unwrap();
        assert!(header_type == 0x14000);

        let _time = stream.read_u32().await.unwrap();

        let meta_type = stream.read_u32().await.unwrap();
        let meta_len = stream.read_u32().await.unwrap();


        let _data_type = stream.read_u32().await.unwrap();
        let data_len = stream.read_u32().await.unwrap();

        let header_close_scope = {
            let mut header_close_scope = [0, 0, 0, 0];
            stream.read_exact(&mut header_close_scope).await.unwrap();
            header_close_scope
        };
        assert!(header_close_scope == DF01_CLOSE_SCOPE);

        let meta_bytes = {
            let mut meta_bytes = vec![0u8; meta_len as usize];
            stream.read_exact(&mut meta_bytes[..]).await.unwrap();
            meta_bytes
        };


        let meta_string = String::from_utf8(meta_bytes).unwrap();
        let meta: DFMeta = match MetaType::try_from(meta_type).unwrap() {
            MetaType::Json => {
                serde_json::from_str(&meta_string).unwrap()
            },
            meta_type => panic!("MetaType::{meta_type:?} handling is not implemented")
        };

        let data = if(data_len != 0) {
            let mut data_bytes = vec![0u8; data_len as usize];
            stream.read_exact(&mut data_bytes[..]).await.unwrap();
            Some(data_bytes)
        } else {
            None
        };

        DFMessage { meta, data }
    } else {
        // elif header_type == b"#~DF02":
        //     header['type'] = header_type[2:6]
        //     header['meta_type'] = stream.read(2)
        //     header['meta_len'] = struct.unpack('>I', stream.read(4))[0]
        //     header['data_len'] = struct.unpack('>I', stream.read(4))[0]
        //     stream.read(4)

        panic!("unsupported opening scope {:?}", header_open_scope)
    }

}


pub async fn push_df_message(
    stream: & mut (impl AsyncWriteExt + std::marker::Unpin), 
    meta: DFMeta, data: Option<Vec<u8>>) {

        let meta_vec =  serde_json::to_vec_pretty(&meta).unwrap();

        stream.write(&DF01_OPEN_SCOPE).await.unwrap();
        stream.write_u32(0x00014000).await.unwrap();
        stream.write_u32(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32).await.unwrap();
        stream.write_u32(MetaType::Json as u32).await.unwrap();
        stream.write_u32(meta_vec.len() as u32).await.unwrap();
        stream.write_u32(0x00000000).await.unwrap();
        if let Some(bytes) = &data {
            stream.write_u32(bytes.len() as u32).await.unwrap();
        } else {
            stream.write_u32(0 as u32).await.unwrap();
        }
        stream.write(&DF01_CLOSE_SCOPE).await.unwrap();


        stream.write(&meta_vec[..]).await.unwrap();
        if let Some(bytes) = data {
            stream.write(&bytes[..]).await.unwrap();
        }

        stream.flush().await.unwrap();

}

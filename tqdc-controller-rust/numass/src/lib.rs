pub mod protos;

use dataforge::DFBinaryHeader;
use serde_with::{serde_as, DisplayFromStr, PickFirst};

use std::fmt::Debug;
use std::vec;

use chrono::NaiveDateTime;
use serde::{Serialize, Deserialize};
use serde_repr::{Serialize_repr, Deserialize_repr};
use tokio::io::{AsyncReadExt};

#[derive(Debug, Serialize_repr, Deserialize_repr, Clone)]
#[repr(u32)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum DFMeta {
    #[serde(rename="command")]
    Command(Command),
    #[serde(rename="reply")]
    Reply(Reply),
    #[serde(rename="info_file")]
    InfoFile {

    },
    #[serde(rename="voltage")]
    Voltage {

    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ReplyStatus {
    #[serde(rename="ok")]
    Ok
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct ZeroSuppressionParams {
    pub baseline: usize,
    pub threshold: i16
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
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
        #[serde_as(as = "PickFirst<(_, DisplayFromStr)>")]
        acquisition_time: f32,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
        external_meta: Option<serde_json::Value>,
        config: Option<serde_json::Value>,
        zero_suppression: Option<ZeroSuppressionParams>,
        // split: bool,
        status: ReplyStatus,
    }
}

// TODO: handle errors
// TODO: make generic and move to dataforge package
pub async fn extract_df_message(stream: & mut (impl AsyncReadExt + std::marker::Unpin)) -> tokio::io::Result<DFMessage> {

    let header = dataforge::read_binary_header(stream).await?;
    
    match header {
        DFBinaryHeader::DF01 {  meta_type, meta_len, data_len, .. } => {
            let meta_bytes = {
                let mut meta_bytes = vec![0u8; meta_len as usize];
                stream.read_exact(&mut meta_bytes[..]).await?;
                meta_bytes
            };
            // let meta_string = String::from_utf8(meta_bytes).unwrap();

            let meta: DFMeta = match meta_type {
                dataforge::MetaType::Json => {
                    serde_json::from_slice(&meta_bytes).unwrap()
                },
                meta_type => panic!("MetaType::{meta_type:?} handling is not implemented")
            };
    
            let data = if data_len != 0 {
                let mut data_bytes = vec![0u8; data_len as usize];
                stream.read_exact(&mut data_bytes[..]).await?;
                Some(data_bytes)
            } else {
                None
            };
    
            Ok(DFMessage { meta, data })
        },
        DFBinaryHeader::Text => todo!()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn parse_2021_file() {

        let mut file = tokio::fs::File::open(
            "../test-data/points/p0(30s)(HV1=14000).df"
        ).await.unwrap();

        let msg = extract_df_message(&mut file).await.unwrap();
        println!("{:?}", msg.meta)
    }
    
    #[tokio::test]
    async fn parse_2022_file() {

        let mut file = tokio::fs::File::open(
            "../test-data/points/p-2022-11-14-real-5v.df"
        ).await.unwrap();

        let msg = extract_df_message(&mut file).await.unwrap();
        println!("{:?}", msg.meta)
    }
}
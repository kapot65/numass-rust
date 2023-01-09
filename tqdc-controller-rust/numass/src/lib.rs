pub mod protos;

use serde_with::{serde_as, DisplayFromStr, PickFirst};
use std::fmt::Debug;

use chrono::NaiveDateTime;
use serde::{Serialize, Deserialize};
use serde_repr::{Serialize_repr, Deserialize_repr};

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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum NumassMeta {
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
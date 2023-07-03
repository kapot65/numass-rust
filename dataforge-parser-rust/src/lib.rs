use std::error::Error;

use arrayref::array_ref;
use  byteorder::BigEndian;
#[cfg(feature = "tokio")] 
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use serde::{Serialize, Deserialize};

const DF01_OPEN_SCOPE: &[u8; 2] = b"#!";
const DF01_CLOSE_SCOPE: &[u8; 4] = b"!#\r\n";

const DF02_OPEN_SCOPE: &[u8; 2] = b"#~";
// const DF02_CLOSE_SCOPE: &[u8; 4] = b"~#\r\n";

const DF01_METADATA_ENDING: &[u8; 2] = b"\r\n";

#[derive(Debug)]
pub enum MetaType {
    Undefined = 0x00000000,
    Json = 0x00010000,
    Qdatastream = 0x00010007,
}

#[derive(Debug)]
pub enum DFParseError {
    NotADFMessage(String),
    MalformedHeader(String),
    Unimplemented(String),
    MetaError(Box<dyn Error + std::marker::Send>),
    IoError(std::io::Error)
}

impl std::fmt::Display for DFParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotADFMessage(desc) => f.write_str(desc),
            Self::MalformedHeader(desc) => f.write_str(desc),
            Self::Unimplemented(desc) => f.write_str(desc),
            Self::MetaError(err) => err.fmt(f),
            Self::IoError(err) => err.fmt(f)
        }
    }
}

impl Error for DFParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NotADFMessage(_) => None, 
            Self::MalformedHeader(_) => None,
            Self::Unimplemented(_) => None,
            Self::MetaError(err) => err.source(),
            Self::IoError(err) => err.source()
        }
    }
}

impl From<std::io::Error> for DFParseError {
    fn from(err: std::io::Error) -> Self {
        DFParseError::IoError(err)
    }
}

impl From<serde_json::Error> for DFParseError {
    fn from(err: serde_json::Error) -> Self {
        DFParseError::MetaError(Box::new(err))
    }
}

impl TryFrom<u32> for MetaType {
    type Error = DFParseError;
    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v {
            code if code == MetaType::Undefined as u32 => Ok(MetaType::Undefined),
            code if code == MetaType::Json as u32 => Ok(MetaType::Json),
            code if code == MetaType::Qdatastream as u32 => Ok(MetaType::Qdatastream),
            code => Err(DFParseError::MalformedHeader(
                format!("No meta type for 0x{code:x} code!")
            ))
        }
    }
}

#[derive(Debug)]
pub enum DFBinaryHeader {
    DF01 {
        time: u32,
        meta_type: MetaType,
        meta_len: usize,
        data_type: u32,
        data_len: usize
    },
}

impl DFBinaryHeader {
    fn get_meta_len(&self) -> usize {
        match self {
            Self::DF01 { meta_len, .. } => *meta_len
        }
    }

    fn get_data_len(&self) -> usize {
        match self {
            Self::DF01 { data_len, .. } => *data_len
        }
    }
}

#[derive(Debug)]
pub struct DFMessage<T: for<'a> Deserialize<'a>> {
    pub meta: T,
    pub data: Option<Vec<u8>>
}

fn header_size(scope: &[u8; 2]) -> Result<usize, DFParseError> {
    if scope == DF01_OPEN_SCOPE {
        Ok(30)
    } else if scope == DF02_OPEN_SCOPE {
        Ok(24) // TODO: check
    } else {
        Err(DFParseError::NotADFMessage(
            format!("unsupported opening scope {}", String::from_utf8_lossy(scope))
        ))
    }
}

fn parse_header(header_bytes: &[u8; 30]) -> Result<DFBinaryHeader, DFParseError> {
    
    if &header_bytes[0..2] == DF01_OPEN_SCOPE {
        let header_type = u32::from_be_bytes(*array_ref![header_bytes, 2, 4]);
        if header_type != 0x14000 {
            Err(DFParseError::MalformedHeader("header_type != 0x14000".to_string()))?
        }

        let time = u32::from_be_bytes(*array_ref![header_bytes, 6, 4]);

        let meta_type = MetaType::try_from(
            u32::from_be_bytes(*array_ref![header_bytes, 10, 4])
        )?;
        let meta_len = u32::from_be_bytes(*array_ref![header_bytes, 14, 4]) as usize;

        let data_type = u32::from_be_bytes(*array_ref![header_bytes, 18, 4]);
        let data_len = u32::from_be_bytes(*array_ref![header_bytes, 22, 4]) as  usize;

        if &header_bytes[26..30] != DF01_CLOSE_SCOPE {
            Err(DFParseError::MalformedHeader(
                format!(
                    "open scope '{}' does not match closing '{}'",
                    String::from_utf8_lossy(&header_bytes[0..2]),
                    String::from_utf8_lossy(&header_bytes[26..30])
                )
            ))?
        }

        Ok(DFBinaryHeader::DF01 { 
            time,
            meta_type,
            meta_len,
            data_type,
            data_len
         })
    } else if &header_bytes[0..2] == DF02_OPEN_SCOPE {
        Err(DFParseError::Unimplemented("DF02 format parsing is not implemented".to_string()))?
    } else {
        Err(DFParseError::NotADFMessage(
            format!("unsupported opening scope {}", String::from_utf8_lossy(&header_bytes[0..2]))
        ))
    }
}

fn make_message<T: Serialize>(meta: T, data: &Option<Vec<u8>>) -> Result<Vec<u8>, DFParseError> {
    
    let meta_vec =  {
        let mut json = serde_json::to_vec_pretty(&meta).unwrap();
        json.extend(DF01_METADATA_ENDING);
        json
    };
    
    let capacity = {
        let capacity = 30 + meta_vec.len();
        if let Some(data) = &data {
            capacity + data.len()
        } else {
            capacity
        }
    };

    let mut buffer = Vec::with_capacity(capacity);

    std::io::Write::write_all(&mut buffer, DF01_OPEN_SCOPE)?;
    byteorder::WriteBytesExt::write_u32::<BigEndian>(&mut buffer, 0x00014000)?;
    byteorder::WriteBytesExt::write_u32::<BigEndian>(&mut buffer, std::time::SystemTime::now().duration_since(
        std::time::UNIX_EPOCH).unwrap().as_secs() as u32)?;
    byteorder::WriteBytesExt::write_u32::<BigEndian>(&mut buffer, MetaType::Json as u32)?;
    byteorder::WriteBytesExt::write_u32::<BigEndian>(&mut buffer, meta_vec.len() as u32)?;
    byteorder::WriteBytesExt::write_u32::<BigEndian>(&mut buffer, 0x00000000)?;
    if let Some(bytes) = &data {
        byteorder::WriteBytesExt::write_u32::<BigEndian>(&mut buffer, bytes.len() as u32)?;
    } else {
        byteorder::WriteBytesExt::write_u32::<BigEndian>(&mut buffer, 0)?;
    }
    std::io::Write::write_all(&mut buffer, DF01_CLOSE_SCOPE)?;
    
    std::io::Write::write_all(&mut buffer, &meta_vec)?;

    Ok(buffer)
}


pub fn read_binary_header_sync(stream: & mut (impl std::io::Read + std::marker::Unpin)) -> Result<DFBinaryHeader, DFParseError> {

    let mut header_bytes = [0u8; 30];

    stream.read_exact(&mut header_bytes[0..2])?;
    let header_size = header_size(array_ref![header_bytes, 0, 2])?;

    stream.read_exact(&mut header_bytes[2..header_size])?;

    parse_header(&header_bytes)
}

#[cfg(feature = "tokio")] 
pub async fn read_binary_header(stream: & mut (impl AsyncReadExt + std::marker::Unpin)) -> Result<DFBinaryHeader, DFParseError> {

    let mut header_bytes = [0u8; 30];

    stream.read_exact(&mut header_bytes[0..2]).await?;
    let header_size = header_size(array_ref![header_bytes, 0, 2])?;

    stream.read_exact(&mut header_bytes[2..header_size]).await?;

    parse_header(&header_bytes)
}

pub fn write_df_message_sync<T: Serialize>(
    stream: & mut (impl std::io::Write + std::marker::Unpin), 
    meta: T, data: Option<Vec<u8>>) -> Result<(), DFParseError> {

        stream.write_all(&make_message(meta, &data)?)?;
        if let Some(bytes) = data {
            stream.write_all(&bytes)?;
        }
        Ok(())
}

#[cfg(feature = "tokio")] 
pub async fn write_df_message<T: Serialize>(
    stream: & mut (impl AsyncWriteExt + std::marker::Unpin), 
    meta: T, data: Option<Vec<u8>>) -> Result<(), DFParseError> {

        stream.write_all(&make_message(meta, &data)?).await?;
        if let Some(bytes) = data {
            stream.write_all(&bytes).await?;
        }
        Ok(())
}

pub fn parse_meta<T: for<'a> Deserialize<'a>> (
    header: &DFBinaryHeader, meta_bytes: Vec<u8>) -> Result<T, DFParseError> {
    
    let meta = match &header {
        DFBinaryHeader::DF01 {  meta_type, .. } => {
            match meta_type {
                MetaType::Json => {
                    serde_json::from_slice(&meta_bytes)?
                },
                meta_type => {
                    Err(DFParseError::Unimplemented(format!("MetaType::{meta_type:?} handling is not implemented")))?
                }
            }
        },
    };

    Ok(meta)
}

pub fn read_df_header_and_meta_sync<T: for<'a> Deserialize<'a>> (
    stream: & mut (impl std::io::Read + std::marker::Unpin)) -> Result<(DFBinaryHeader, T), DFParseError> {

    let header = read_binary_header_sync(stream)?;

    let mut meta_bytes = vec![0u8; header.get_meta_len()];
    stream.read_exact(&mut meta_bytes[..])?;
    
    let meta = parse_meta(&header, meta_bytes)?;

    Ok((header, meta))
}

#[cfg(feature = "tokio")]
pub async fn read_df_header_and_meta<T: for<'a> Deserialize<'a>> (
    stream: & mut (impl AsyncReadExt + std::marker::Unpin)) -> Result<(DFBinaryHeader, T), DFParseError> {
        let header = read_binary_header(stream).await?;

        let mut meta_bytes = vec![0u8; header.get_meta_len()];
        stream.read_exact(&mut meta_bytes[..]).await?;
        
        let meta = parse_meta(&header, meta_bytes)?;

        Ok((header, meta))
}

pub fn read_df_message_sync<T: for<'a> Deserialize<'a>> (
    stream: & mut (impl std::io::Read + std::marker::Unpin)) -> Result<DFMessage<T>, DFParseError> {

    let (header, meta) = read_df_header_and_meta_sync(stream)?;
    
    let data = if header.get_data_len() != 0 {
        let mut data_bytes = vec![0u8; header.get_data_len()];
        stream.read_exact(&mut data_bytes[..])?;
        Some(data_bytes)
    } else { None };

    Ok(DFMessage { meta, data })
}

#[cfg(feature = "tokio")]
pub async fn read_df_message<T: for<'a> Deserialize<'a>> (
    stream: & mut (impl AsyncReadExt + std::marker::Unpin)) -> Result<DFMessage<T>, DFParseError> {

        let (header, meta) = read_df_header_and_meta(stream).await?;
    
        let data = if header.get_data_len() != 0 {
            let mut data_bytes = vec![0u8; header.get_data_len()];
            stream.read_exact(&mut data_bytes[..]).await?;
            Some(data_bytes)
        } else { None };
    
        Ok(DFMessage { meta, data })
}
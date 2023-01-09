use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Serialize, Deserialize};

const DF01_OPEN_SCOPE: &[u8; 2] = b"#!";
const DF01_CLOSE_SCOPE: &[u8; 4] = b"!#\r\n";
const DF01_METADATA_ENDING: &[u8; 2] = b"\r\n";

#[derive(Debug)]
pub enum MetaType {
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

#[derive(Debug)]
pub enum DFBinaryHeader {
    DFText,
    DF01 {
        time: u32,
        meta_type: MetaType,
        meta_len: usize,
        data_type: u32,
        data_len: usize
    }
}

#[derive(Debug)]
pub struct DFMessage<T: for<'a> Deserialize<'a>> {
    pub meta: T,
    pub data: Option<Vec<u8>>
}

pub async fn read_binary_header(stream: & mut (impl AsyncReadExt + std::marker::Unpin)) -> tokio::io::Result<DFBinaryHeader> {

    let header_open_scope = {
        let mut header_open_scope = [0, 0];
        stream.read_exact(&mut header_open_scope).await?;
        header_open_scope
    };
    
    if header_open_scope == *DF01_OPEN_SCOPE {
        
        let header_type = stream.read_u32().await?;
        assert!(header_type == 0x14000);

        let time = stream.read_u32().await?;

        let meta_type = MetaType::try_from(stream.read_u32().await?).unwrap(); // TODO: propagate unwrap
        let meta_len = stream.read_u32().await? as usize;

        let data_type = stream.read_u32().await?;
        let data_len = stream.read_u32().await? as usize;

        let header_close_scope = {
            let mut header_close_scope = [0, 0, 0, 0];
            stream.read_exact(&mut header_close_scope).await?;
            header_close_scope
        };
        assert!(header_close_scope == *DF01_CLOSE_SCOPE);

        Ok(DFBinaryHeader::DF01 { 
            time,
            meta_type,
            meta_len,
            data_type,
            data_len
         })
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

pub async fn write_df_message<T: Serialize>(
    stream: & mut (impl AsyncWriteExt + std::marker::Unpin), 
    meta: T, data: Option<Vec<u8>>) -> tokio::io::Result<()> {

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

        assert!(buffer.write(DF01_OPEN_SCOPE).await? == 2);
        buffer.write_u32(0x00014000).await?;
        buffer.write_u32(std::time::SystemTime::now().duration_since(
            std::time::UNIX_EPOCH).unwrap().as_secs() as u32).await?;
        buffer.write_u32(MetaType::Json as u32).await?;
        buffer.write_u32(meta_vec.len() as u32).await?;
        buffer.write_u32(0x00000000).await?;
        if let Some(bytes) = &data {
            buffer.write_u32(bytes.len() as u32).await?;
        } else {
            buffer.write_u32(0).await?;
        }
        assert!(buffer.write(DF01_CLOSE_SCOPE).await? == 4);

        // TODO: make extend without copy (concat?)
        buffer.extend(meta_vec);
        if let Some(bytes) = data {
            buffer.extend(bytes);
        }

        stream.write_all(&buffer[..]).await?;
        stream.flush().await?;

        Ok(())
}

pub async fn read_df_header_and_meta<T: for<'a> Deserialize<'a>> (
    stream: & mut (impl AsyncReadExt + std::marker::Unpin)) -> tokio::io::Result<(DFBinaryHeader, T)> {

    let header = read_binary_header(stream).await?;
    
    let meta = match &header {
        DFBinaryHeader::DF01 {  meta_type, meta_len, .. } => {
            let meta_bytes = {
                let mut meta_bytes = vec![0u8; meta_len.to_owned() as usize];
                stream.read_exact(&mut meta_bytes[..]).await?;
                meta_bytes
            };

            match meta_type {
                MetaType::Json => {
                    serde_json::from_slice(&meta_bytes)?
                },
                meta_type => panic!("MetaType::{meta_type:?} handling is not implemented")
            }
        },
        DFBinaryHeader::DFText => todo!()
    };

    Ok((header, meta))
}

pub async fn read_df_message<T: for<'a> Deserialize<'a>> (
    stream: & mut (impl AsyncReadExt + std::marker::Unpin)) -> tokio::io::Result<DFMessage<T>> {

    let (header, meta) = read_df_header_and_meta(stream).await?;
    
    let data = match header {
        DFBinaryHeader::DF01 { data_len, .. } => {
            if data_len != 0 {
                let mut data_bytes = vec![0u8; data_len as usize];
                stream.read_exact(&mut data_bytes[..]).await?;
                Some(data_bytes)
            } else {
                None
            }
        },
        DFBinaryHeader::DFText => todo!()
    };

    Ok(DFMessage { meta, data })
}
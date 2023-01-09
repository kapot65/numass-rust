use tokio::io::AsyncReadExt;

use dataforge::{read_binary_header, DFBinaryHeader};

#[tokio::main]
async fn main() {

    let mut file = tokio::fs::File::open("./test-data/points/p0(30s)(HV1=14000).df").await.unwrap();

    let header = read_binary_header(&mut file).await.unwrap();

    if let DFBinaryHeader::DF01 { meta_len, .. } = header {

        let mut meta_bytes = vec![0u8; meta_len as usize];
        file.read_exact(&mut meta_bytes[..]).await.unwrap();

        println!("{}", String::from_utf8(meta_bytes).unwrap());
    }  
}
use dataforge::{read_binary_header, DFBinaryHeader};

#[cfg(not(feature = "tokio"))]
fn main() {
    use std::io::Read;

    let mut file = std::fs::File::open("./resources/test/df01-point.df").unwrap();

    let header = read_binary_header(&mut file).unwrap();

    match header {
        DFBinaryHeader::DF01 { meta_len, .. } => {

            let mut meta_bytes = vec![0u8; meta_len];
            file.read_exact(&mut meta_bytes[..]).unwrap();

            println!("{}", String::from_utf8(meta_bytes).unwrap());
        }
    }  
}

#[cfg(feature = "tokio")]
#[tokio::main]
async fn main() {

    use tokio::io::AsyncReadExt;

    let mut file = tokio::fs::File::open("./resources/test/df01-point.df").await.unwrap();

    let header = read_binary_header(&mut file).await.unwrap();

    match header {
        DFBinaryHeader::DF01 { meta_len, .. } => {

            let mut meta_bytes = vec![0u8; meta_len];
            file.read_exact(&mut meta_bytes[..]).await.unwrap();

            println!("{}", String::from_utf8(meta_bytes).unwrap());
        }
    }
}
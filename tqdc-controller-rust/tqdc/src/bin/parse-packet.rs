use tqdc::mlink;
use tokio::io::{AsyncReadExt};

#[tokio::main]
async fn main() -> tokio::io::Result<()> {

    let mut file = tokio::fs::File::open(
        // "./test-data/frames-0l.bin"
        "./test-data/frames-subs-overflow.bin"
        // "./test-data/frames-trash-3.bin"
        // "./test-data/frames-0l-cropped-2.bin"
        // "./test-data/frames-408ns-7ch.bin"
    ).await?;

    let mut contents = [0u8; 2048];
    let size = file.read(&mut contents).await?;

    let message = mlink::MlinkMessage::from_datagram(&contents[..size]);
    
    println!("{message:?}");

    Ok(())
}
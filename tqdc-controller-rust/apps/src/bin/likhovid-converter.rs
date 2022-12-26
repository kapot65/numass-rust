use protobuf::Message;
use clap::Parser;

use dataforge::protos::rsb_event;
use tokio::io::AsyncWriteExt;


#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Opt {
    filepath: std::path::PathBuf,
}

#[tokio::main]
async fn main() {

    let args = Opt::parse();

    let mut point_file = tokio::fs::File::open(&args.filepath).await.unwrap();
    let message = dataforge::extract_df_message(&mut point_file).await.unwrap();

    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();


    let mut out_file = tokio::fs::File::create(args.filepath.to_str().unwrap().to_owned() + ".bin").await.unwrap();

    for channel in point.channels {
        for block in channel.blocks {
            for frame in block.frames {
                out_file.write_u64_le(frame.time).await.unwrap();
                let bytes_written = out_file.write(&[channel.id as u8]).await.unwrap();
                assert!(bytes_written == 1);
                let bytes_written = out_file.write(&frame.data).await.unwrap();
                // println!("{bytes_written}");
                assert!(bytes_written == 150);
            }
        }
    }

    out_file.flush().await.unwrap();
}
use protobuf::Message;
use clap::Parser;

use dataforge::protos::rsb_event;
use tokio::io::AsyncWriteExt;

/// Converts DataForge point to raw binary format.
/// Binary format: [time u64 le (2 bytes), channel_number u8 le (1 byte), waveform i16 le (150 bytes)]
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Opt {
    /// Path to DataForge point
    filepath: std::path::PathBuf,

    /// Output path (if not set "${FILEPATH}.bin" will be used)
    #[clap(short, long)]
    output: Option<std::path::PathBuf>
}

#[tokio::main]
async fn main() {

    let args = Opt::parse();

    let output_filepath = if let Some(filepath) = args.output {
        filepath.to_str().unwrap().to_owned()
    } else {
        args.filepath.to_str().unwrap().to_owned() + ".bin"
    };

    let mut point_file = tokio::fs::File::open(&args.filepath).await.unwrap();
    let message = dataforge::extract_df_message(&mut point_file).await.unwrap();

    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();


    let mut out_file = tokio::fs::File::create(output_filepath).await.unwrap();

    for channel in point.channels {
        for block in channel.blocks {
            for frame in block.frames {
                out_file.write_u64_le(frame.time).await.unwrap();
                let bytes_written = out_file.write(&[channel.id as u8]).await.unwrap();
                assert!(bytes_written == 1);
                let bytes_written = out_file.write(&frame.data).await.unwrap();
                assert!(bytes_written == 150);
            }
        }
    }

    out_file.flush().await.unwrap();
}
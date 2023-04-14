use std::{collections::BTreeMap, io::Write};

use clap::Parser;
use protobuf::Message;

use dataforge::{read_df_message_sync};
use processing::numass::{protos::rsb_event, NumassMeta};

/// Converts DataForge point to raw binary format.
/// Binary format: [time u64 le (2 bytes), channel_number u8 le (1 byte), waveform i16 le (150 bytes)]
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Opt {
    /// Path to DataForge point
    filepath: std::path::PathBuf,

    /// Output path (if not set "${FILEPATH}.bin" will be used)
    #[clap(short, long)]
    output: Option<std::path::PathBuf>,

    /// convert to ascii tsv format instead of binary
    #[clap(long)]
    ascii: bool
}

fn frame_to_waveform(data: &[u8]) -> Vec<i16> {
    let waveform_len = data.len() / 2;
    (0..waveform_len)
        .map(|idx| i16::from_le_bytes(data[idx * 2..idx * 2 + 2].try_into().unwrap()))
        .collect::<Vec<_>>()
}

fn main() {
    let args = Opt::parse();

    let output_filepath = if let Some(filepath) = args.output {
        filepath.to_str().unwrap().to_owned()
    } else if args.ascii {
        args.filepath.to_str().unwrap().to_owned() + ".tsv"
    } else {
        args.filepath.to_str().unwrap().to_owned() + ".bin"
    };

    let mut point_file = std::fs::File::open(&args.filepath).unwrap();
    let message = read_df_message_sync::<NumassMeta>(&mut point_file).unwrap();

    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

    let mut frames_sorted = BTreeMap::new();

    for channel in point.channels {
        for block in channel.blocks {
            for frame in block.frames {
                let group = frames_sorted
                    .entry(frame.time).or_insert(BTreeMap::new());
                group.insert(channel.id as u8, frame.data);
            }
        }
    }

    let mut out_file = std::fs::File::create(output_filepath).unwrap();
    if args.ascii {
        out_file.write_all("time\tchannel\t".as_bytes()).unwrap();
        for idx in 0..75 {
            out_file.write_all(format!("{idx}\t").as_bytes()).unwrap();
        }
        out_file.write_all("\n".as_bytes()).unwrap();
     }

    for (time, pixels) in frames_sorted {

        for (ch_id, data) in pixels {
            if args.ascii {
                out_file.write_all(format!("{time}\t{ch_id}\t").as_bytes()).unwrap();
                let waveform = frame_to_waveform(&data);
                waveform.iter().for_each(|bin| {
                    out_file.write_all(format!("{bin}\t").as_bytes()).unwrap();
                });
                out_file.write_all("\n".as_bytes()).unwrap();
            } else {
                out_file.write_all(&time.to_le_bytes()).unwrap();
                out_file.write_all(&ch_id.to_le_bytes()).unwrap();
                out_file.write_all(&data).unwrap();
            }
        }
    }

    out_file.flush().unwrap();
}
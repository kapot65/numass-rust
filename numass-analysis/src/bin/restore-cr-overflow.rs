use std::{collections::BTreeSet, io::Write};

use dataforge::read_df_message_sync;
use processing::numass::{protos::rsb_event, NumassMeta};
use protobuf::Message;

fn main() {
    
    // let filepath = "/data-nvme/2023_11/Tritium_corr/set_27/p0(10s)(HV1=10000)";
    // let filepath = "/data-nvme/2023_11/Tritium_corr/set_27/p1(10s)(HV1=10500)";
    // let filepath = "/data-nvme/2023_11/Tritium_corr/set_27/p2(10s)(HV1=11000)";
    // let filepath = "/data-nvme/2023_11/Tritium_corr/set_27/p3(10s)(HV1=11500)";
    // let filepath = "/data-nvme/2023_11/Tritium_corr/set_27/p4(10s)(HV1=12000)";
    // let filepath = "/data-nvme/2023_11/Tritium_corr/set_27/p5(10s)(HV1=12500)";
    // let filepath = "/data-nvme/2023_11/Tritium_corr/set_27/p6(10s)(HV1=13000)";
    // let filepath = "/data-nvme/2023_11/Tritium_corr/set_27/p7(10s)(HV1=13500)";
    // let filepath = "/data-nvme/2023_11/Tritium_corr/set_27/p8(10s)(HV1=14000)";
    let filepath = "/data-nvme/2023_11/Tritium_corr/set_27/p9(10s)(HV1=14500)";
    // let filepath = "/data-nvme/2023_11/Tritium_corr/set_27/p10(10s)(HV1=15000)";

                                          
    let point = {
        let mut point_file = std::fs::File::open(filepath).unwrap();
        let message = read_df_message_sync::<NumassMeta>(&mut point_file).unwrap();
        rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap()
    };

    let times = point.channels.iter().flat_map(|channel| {
        channel.blocks.iter().flat_map(|block| {
            block.frames.iter().map(|frame| {
                frame.time
                // 18446744070000000000
            })
        })
    }).collect::<BTreeSet<_>>();

    {
        let mut file = std::fs::File::create("amplitudes.csv").unwrap();
        times.iter().for_each(|time| {
            file.write_all(format!("{time}\n").as_bytes()).unwrap();
        });
    }
}
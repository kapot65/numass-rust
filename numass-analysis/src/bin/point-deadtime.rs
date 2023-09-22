use dataforge::read_df_message;
use processing::numass::{protos::rsb_event, NumassMeta};
use protobuf::Message;

#[tokio::main]
async fn main() {
    
    let filepath = "/data-nvme/2023_03/Tritium_2/set_1/p118(30s)(HV1=12000)";
    // let filepath = "/data-nvme/2023_03/Tritium_2/set_1/p52(30s)(HV1=15000)";
                                          
    let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
    let message = read_df_message::<NumassMeta>(&mut point_file)
        .await
        .unwrap();

    let params = processing::ProcessParams::default();
    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

    let amplitudes = processing::extract_amplitudes(&point, &params);

    let amplitudes =  amplitudes.into_keys().collect::<Vec<_>>();

    let mut total_deadtime = 0;
    let mut close_count = 0;
    let mut normal_count = 0;
    let mut triple_close_count = 0;

    amplitudes.windows(3).for_each(|pair| {

        let time_delta = pair[2] - pair[1];
        let time_delta_prev = pair[1] - pair[0];
        
        if time_delta_prev < 2400 && time_delta < 2400 {
            // total_deadtime += 2400;
            triple_close_count += 1;
        } else if time_delta < 2400 {
            total_deadtime += 1600;
            close_count += 1;
        } else {
            total_deadtime += 800;
            normal_count += 1;
        }                                                                                                                  
    });

    println!("total deadtime: {total_deadtime} ns ({close_count} close, {normal_count} normal, {triple_close_count} triple)");
}
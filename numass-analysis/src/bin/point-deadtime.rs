//! # Point Deadtime Calculation
//! Ручное вычисление мертвого времени точки
//! > [!NOTE]  
//! > Устаревший скрипт, но мб пригодится.
//! TODO: обновить или удалить
use std::path::PathBuf;

use processing::{process::ProcessParams, storage::process_point};

#[tokio::main]
async fn main() {
    
    let filepath = PathBuf::from("/data-nvme/2023_03/Tritium_2/set_1/p118(30s)(HV1=12000)");
    // let filepath = "/data-nvme/2023_03/Tritium_2/set_1/p52(30s)(HV1=15000)";
                                          
    let (events, _) = process_point(&filepath, &ProcessParams::default(), None).await.unwrap().1.unwrap();

    let events =  events.into_keys().collect::<Vec<_>>();

    let mut total_deadtime = 0;
    let mut close_count = 0;
    let mut normal_count = 0;
    let mut triple_close_count = 0;

    events.windows(3).for_each(|pair| {

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
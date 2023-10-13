// Вычисление отношений полных тритиевых спектров для сетов с разной интенсивностью.
// Повторение результата из https://disk.yandex.ru/i/YCD3gtCozpEwBg

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use analysis::{get_points_by_pattern, workspace::{get_db_fast_root, get_workspace}};
use dataforge::read_df_message;
use processing::{numass::{NumassMeta, protos::rsb_event}, extract_events, ProcessParams, PostProcessParams, post_process};
use protobuf::Message;
use tokio::sync::Mutex;


async fn calc_count_rates(groups: BTreeMap<u16, Vec<PathBuf>>, process_params: ProcessParams, post_process_params: PostProcessParams) -> BTreeMap<u16, BTreeMap<usize, f32>> {

    let count_rates = Arc::new(Mutex::new(BTreeMap::new()));

    let handles = groups.into_iter().map(|(u, points)| {

        let count_rates = Arc::clone(&count_rates);

        tokio::spawn(async move {

            let mut counts = BTreeMap::new();
            let mut acq_time = 0.0; 

            for filepath in points {

                let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
                let message = read_df_message::<NumassMeta>(&mut point_file)
                    .await
                    .unwrap();
                let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

                let amps = extract_events(&point, &process_params);
                let amps = post_process(amps, &post_process_params);

                for (_, amps) in amps {
                    for (ch, (_, amp)) in amps {
                        if (2.0..20.0).contains(&amp) {
                            *counts.entry(ch).or_insert(0.0) += 1.0;
                        }
                    }
                }

                acq_time += 30.0;
            }
            
            count_rates.lock().await.insert(u, 
                counts.into_iter().map(|(ch, count)| (ch, count / acq_time)).collect::<BTreeMap<_, _>>()
            );
        })

        
    });

    for handle in handles {
        handle.await.unwrap();
    }
    
    let cr = count_rates.lock().await.clone();
    cr
}

#[tokio::main]
async fn main() {

    let db_root = get_db_fast_root().to_str().unwrap().to_owned();

    let ethalon = get_points_by_pattern(&db_root, "/2023_03/Tritium_1/set_[12]/p*", &[]);
    
    let sample = get_points_by_pattern(&db_root, "/2023_03/Tritium_1/set_[89]/p*", &[]);

    let process_params = ProcessParams::default();
    let post_process_params = PostProcessParams::default();

    let eth_cr = calc_count_rates(ethalon, process_params, post_process_params);
    let sample_cr = calc_count_rates(sample, process_params, post_process_params).await;
    let eth_cr = eth_cr.await;

    let out_folder = get_workspace().join("spectrum-ratio-temp");
    std::fs::create_dir_all(&out_folder).unwrap();

    let mut eth_tsv = "u\t1\t2\t3\t4\t5\t6\t7\n".to_string();
    for (u, crs) in eth_cr {
        eth_tsv += format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n", u, crs[&0], crs[&1], crs[&2], crs[&3], crs[&4], crs[&5], crs[&6]).as_str();
    }
    tokio::fs::write(out_folder.join("eth.tsv"), eth_tsv).await.unwrap();

    let mut sample_tsv = "u\t1\t2\t3\t4\t5\t6\t7\n".to_string();
    for (u, crs) in sample_cr {
        sample_tsv += format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n", u, crs[&0], crs[&1], crs[&2], crs[&3], crs[&4], crs[&5], crs[&6]).as_str();
    }
    tokio::fs::write(out_folder.join("sample.tsv") , sample_tsv).await.unwrap();

    // let out = itertools::zip_eq(eth_cr.into_iter(), sample_cr.into_iter())
    //     .map(|((u_eth, eth_channels), (u_sample, sample_channels))| {
    //         assert_eq!(u_eth, u_sample);
    //         (u_eth, itertools::zip_eq(eth_channels, sample_channels).map(|((ch, eth_cr), (_, sample_cr))| {
    //             (ch, sample_cr / eth_cr)
    //         }).collect::<BTreeMap<_, _>>())
    //     }).collect::<BTreeMap<_, _>>();

    // let n: usize = 100;
    // let t: Vec<f64> = linspace(0., 10., n).collect();
    // let y = t.iter().map(|x| x.sin()).collect();

    // let mut plot = Plot::new();

    // let trace = Scatter::new(t, y).mode(Mode::Markers);
    // plot.add_trace(trace);
    // if show {
    //     plot.show();
    // }
    // println!("{}", plot.to_inline_html(Some("simple_scatter_plot")));
    
}
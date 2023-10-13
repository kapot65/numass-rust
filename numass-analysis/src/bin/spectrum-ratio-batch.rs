// Вычисление отношений полных тритиевых спектров для сетов с разной интенсивностью.
// Повторение результата из https://disk.yandex.ru/i/YCD3gtCozpEwBg

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use analysis::{get_points_by_pattern, workspace::get_db_fast_root};
use dataforge::read_df_message;
use indicatif::ProgressStyle;
use plotly::{Plot, Scatter, common::{Mode, Title}, Layout, layout::Axis};
use processing::{numass::{NumassMeta, protos::rsb_event}, extract_events, ProcessParams, PostProcessParams, post_process};
use protobuf::Message;
use tokio::sync::Mutex;


async fn calc_count_rates(groups: BTreeMap<u16, Vec<PathBuf>>, process_params: ProcessParams, post_process_params: PostProcessParams) -> BTreeMap<u16, [f64; 7]> {

    let count_rates = Arc::new(Mutex::new(BTreeMap::new()));

    let handles = groups.into_iter().map(|(u, points)| {

        let count_rates = Arc::clone(&count_rates);

        tokio::spawn(async move {

            let mut counts = [0.0; 7];
            let mut acq_time = 0.0; 

            for filepath in points {

                let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
                let message = read_df_message::<NumassMeta>(&mut point_file)
                    .await
                    .unwrap();
                let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

                // processing
                let amps = extract_events(&point, &process_params);
                // post processing
                let amps = post_process(amps, &post_process_params);

                for (_, amps) in amps {
                    for (ch, (_, amp)) in amps {
                        if (2.0..20.0).contains(&amp) {
                            counts[ch] += 1.0;
                        }
                    }
                }
                acq_time += 30.0;
            }

            if acq_time != 0.0 { // ? need zero check?
                (0..7).for_each(|ch| {
                    counts[ch] /= acq_time;
                });
            }

            count_rates.lock().await.insert(u, counts);
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
    let samples = [
        ("Tr1_(8,9)", get_points_by_pattern(&db_root, "/2023_03/Tritium_1/set_[89]/p*", &[])),
        ("Tr1_(18,19)", get_points_by_pattern(&db_root, "/2023_03/Tritium_1/set_1[89]/p*", &[])),
        ("Tr1_(28,29)", get_points_by_pattern(&db_root, "/2023_03/Tritium_1/set_2[89]/p*", &[])),
        ("Tr2_(1,2)", get_points_by_pattern(&db_root, "/2023_03/Tritium_2/set_[12]/p*", &[])),
        ("Tr2_(8,9)", get_points_by_pattern(&db_root, "/2023_03/Tritium_2/set_[89]/p*", &[])),
        ("Tr2_(18,19)", get_points_by_pattern(&db_root, "/2023_03/Tritium_2/set_1[89]/p*", &[])),
        ("Tr2_(28,29)", get_points_by_pattern(&db_root, "/2023_03/Tritium_2/set_2[89]/p*", &[])),
        ("Tr3_(1,2)", get_points_by_pattern(&db_root, "/2023_03/Tritium_3/set_[12]/p*", &[])),
        ("Tr3_(8,9)", get_points_by_pattern(&db_root, "/2023_03/Tritium_3/set_[89]/p*", &[])),
        ("Tr3_(18,19)", get_points_by_pattern(&db_root, "/2023_03/Tritium_3/set_1[89]/p*", &[])),
        ("Tr3_(28,29)", get_points_by_pattern(&db_root, "/2023_03/Tritium_3/set_2[89]/p*", &[])),
        ("Tr4_(1,2)", get_points_by_pattern(&db_root, "/2023_03/Tritium_4/set_[12]/p*", &[])),
        ("Tr4_(8,9)", get_points_by_pattern(&db_root, "/2023_03/Tritium_4/set_[89]/p*", &[])),
        ("Tr4_(18,19)", get_points_by_pattern(&db_root, "/2023_03/Tritium_4/set_1[89]/p*", &[])),
        ("Tr4_(28,29)", get_points_by_pattern(&db_root, "/2023_03/Tritium_4/set_2[89]/p*", &[])),
        ("Tr5_(1,2)", get_points_by_pattern(&db_root, "/2023_03/Tritium_5/set_[12]/p*", &[])),
        ("Tr5_(8,9)", get_points_by_pattern(&db_root, "/2023_03/Tritium_5/set_[89]/p*", &[])),
    ];
    let process_params = ProcessParams::default();
    let post_process_params = PostProcessParams::default();

    let pb = indicatif::ProgressBar::new(samples.len() as u64 + 1);
    pb.set_style(ProgressStyle::with_template("[{elapsed_precise}] {bar} {pos:>7}/{len:7} {msg}")
    .unwrap());
    let pb = Arc::new(Mutex::new(pb));

    let eth_cr = calc_count_rates(ethalon, process_params, post_process_params);

    let handles = samples.into_iter().map(|(label, sample)| {
        let label = label.to_owned();
        let pb = Arc::clone(&pb);
        tokio::spawn(async move {
            let sample_cr = calc_count_rates(sample, process_params, post_process_params).await;
            pb.lock().await.inc(1);
            (label, sample_cr)
        })
    }).collect::<Vec<_>>();
    
    let mut plot = Plot::new();
    let layout = Layout::new()
        .title(Title::new("Ratios with Tr1(1,2)"))
        .x_axis(
            Axis::new().title(Title::new("U_sp, V")
        ))
        .y_axis(Axis::new().title(Title::new("ratio normed at 15 keV")))
        .height(1000);
    
    plot.set_layout(layout);

    let eth_cr = eth_cr.await;
    pb.lock().await.inc(1);

    for handle in handles  {
        let eth_cr = eth_cr.clone();
        let (label, sample_cr) = handle.await.unwrap();

        let ratio_ch6 = itertools::zip_eq(eth_cr.into_iter(), sample_cr.into_iter())
        .map(|((u_eth, eth_channels), (u_sample, sample_channels))| {
            assert_eq!(u_eth, u_sample);
            let mut ratios = [0.0; 7];
            for ch in 0..7 {
                ratios[ch] = sample_channels[ch] / eth_channels[ch];
            }
            (u_eth, ratios)
        })
        .map(|(u, ratios)| (u as f64, ratios[5])).collect::<Vec<_>>();
    
        // normalize coeffs to 15 keV
        let norm_coeff = 1.0 / ratio_ch6.iter().find(|(u, _)| u == &15000.0).unwrap().1;
        let ratio_ch6 = ratio_ch6.into_iter().map(|(u, r)| (u, r * norm_coeff)).collect::<Vec<_>>();

        let (u, cr): (Vec<_>, Vec<_>) = ratio_ch6.into_iter().unzip();

        let trace = Scatter::new(u, cr).mode(Mode::Markers).name(label);
        plot.add_trace(trace);
    }

    plot.show();


    // let out_folder = get_workspace().join("spectrum-ratio-temp");
    // std::fs::create_dir_all(&out_folder).unwrap();
    // println!("{}", plot.to_inline_html(Some("simple_scatter_plot")));
    
}
//! # Trigger Timestamps
//! Вывод в консоль временных меток событий из файла точки данных.
//! (Начиная с 2024_03 плата начала сбоить в таймстампах, скрипт нужен для проверки правильности)

use std::{collections::BTreeSet, path::PathBuf};

use processing::storage::load_point;

#[tokio::main]
async fn main() {
    let point = load_point(&PathBuf::from(
        // "/data-fast/numass-server/2024_11/Tritium_2_1/set_2/p66(30s)(HV1=15850)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_2/p136(30s)(HV1=12700)",
    ))
    .await;

    let timestamps = point
        .channels
        .into_iter()
        .flat_map(|ch| {
            ch.blocks
                .into_iter()
                .flat_map(|block| {
                    block
                        .frames
                        .into_iter()
                        .map(|frame| frame.time)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    timestamps.iter().enumerate().skip(1).for_each(|(i, &ts)| {
        println!("{ts}\t{}", ts - timestamps[i - 1]);
    });
}

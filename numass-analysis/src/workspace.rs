// Здесь заданы все константы, которые используются в бинарниках
// по аналогии с django settings.py
// параметры заданы костантами чтобы их можно было использовать в derive макросах (cached)
// TODO: перенести общие константы из бинарников сюда
// TODO: добавить возможность чтения констант из файла/cli/env

use std::{path::PathBuf, ops::Range};

pub fn get_workspace() -> PathBuf {
    PathBuf::from("/home/chernov/produced/numass-analysis-workspace/")
}

pub fn get_db_fast_root() -> PathBuf {
    PathBuf::from("/data-nvme")
}

pub fn get_db_slow_root() -> PathBuf {
    PathBuf::from("/data/numass-server")
}

pub fn get_hist_range() -> Range<f32> {
    0.0..40.0
}

pub fn get_hist_bins() -> usize {
    400 * 4
}


#[test]
fn test() {
    println!("{:?}", get_workspace());
}


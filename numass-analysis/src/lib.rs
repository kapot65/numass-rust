use std::{collections::BTreeMap, path::{PathBuf, Path}};

use processing::numass::{NumassMeta, Reply};
use serde::Deserialize;

pub mod cache;
pub mod ethalon;
pub mod amps;

pub mod workspace;

pub fn get_points_by_pattern(db_root: &str, pattern: &str, exclude: &[String]) -> BTreeMap<u16, Vec<PathBuf>> {
    let mut points = BTreeMap::new();

    let paths = glob::glob(&format!("{db_root}/{pattern}")).unwrap();

    paths.for_each(|path| {
        if let Ok(filepath) = path {
            let path_str: String = filepath.to_str().unwrap().to_string();
            if exclude.iter().any(|ex| path_str.contains(ex)) {
                return;
            }

            let u_sp = {
                let filepath = filepath.file_name().unwrap().to_str().unwrap();
                filepath[filepath.len() - 6..filepath.len() - 1].parse::<u16>().unwrap()
            };
            points.entry(u_sp).or_insert(vec![]).push(filepath);
        }
    });

    points
}

#[derive(Deserialize, Debug)]
struct SetParams {
    corr_coef:Coeffs
}

#[derive(Deserialize, Debug)]
pub struct Coeffs {
    pub a: f32,
    pub b: f32,
    pub c: f32,
}

pub struct CorrectionCoeffs {
    coeffs: BTreeMap<String, BTreeMap<String, SetParams>>
}

impl CorrectionCoeffs {

    pub fn load(filepath: &str) -> Self {
        let json = std::fs::read(filepath).unwrap();
        let coeffs = serde_json::from_slice(&json).unwrap();

        CorrectionCoeffs {
            coeffs
        }
    }

    pub fn get(&self, fill: &str, set: &str) -> Option<&Coeffs> {
        self.coeffs.get(fill)?.get(set).map(|params| &params.corr_coef)
    }

    pub fn get_by_index(&self, filepath: &Path) -> f32 {

        let (fill, set) = {
            let set_folder = filepath.parent().unwrap();
            (
                set_folder.parent().unwrap().file_name().unwrap().to_str().unwrap(), 
                set_folder.file_name().unwrap().to_str().unwrap()
            )
        };
        
        let Coeffs {a, b, c} = self.get(fill, set).unwrap();

        let x = {
            let filename = filepath.file_name().unwrap().to_str().unwrap();
            filename[1..filename.find('(').unwrap()].parse::<i32>().unwrap() as f32
        };

        a * x.powf(2.0) + b * x + c
    }

    pub fn get_from_meta(&self, filepath: &Path, meta: &NumassMeta) -> f32 {

        let (fill, set) = {
            let set_folder = filepath.parent().unwrap();
            (
                set_folder.parent().unwrap().file_name().unwrap().to_str().unwrap(), 
                set_folder.file_name().unwrap().to_str().unwrap()
            )
        };
        

        // let Coeffs {a, b} = self.get(fill, set).unwrap();

        // if let NumassMeta::Reply(Reply::AcquirePoint { start_time, ..
        // }) = meta {
        //     let secs = start_time.timestamp() + (3600 * 5);
        //     let x = (secs % 1_000_000) as f32;
        //     1.0 / (a * x + b)
        // } else {
        //     panic!("wrong message type")
        // }


        let Coeffs {a, b, c} = self.get(fill, set).unwrap();

        if let NumassMeta::Reply(Reply::AcquirePoint { start_time, ..
        }) = meta {
            let secs = start_time.timestamp() - (3600 * 3);
            let x= (secs % 1_000_000) as f32;
            1.0 / (a * x.powf(2.0) + b * x + c)
        } else {
            panic!("wrong message type")
        }
    }
}
use std::{collections::BTreeMap, path::{PathBuf, Path}};

use processing::numass::protos::rsb_event::Point;
use serde::Deserialize;

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

    pub fn get_for_point(&self, filepath: &Path, point: &Point) -> f32{

        let (fill, set) = {
            let set_folder = filepath.parent().unwrap();
            (
                set_folder.parent().unwrap().file_name().unwrap().to_str().unwrap(), 
                set_folder.file_name().unwrap().to_str().unwrap()
            )
        };

        let Coeffs {a, b} = self.get(fill, set).unwrap();

        let secs = point.channels.first().unwrap().blocks.first().unwrap().time / 
                1_000_000_000 + (3600 * 4);

        1.0 / (a * (secs % 1_000_000) as f32 + b)
    }
}
#![warn(clippy::all, rust_2018_idioms)]

pub mod app;

pub mod filtered_viewer;
pub mod point_viewer;

pub mod backend;

pub const CACHE_DIRECTORY: &str = "CACHE_DIRECTORY";

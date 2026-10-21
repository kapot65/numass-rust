pub extern crate numass;
#[cfg(feature = "polars")]
pub extern crate polars;
pub mod histogram;
pub mod viewer; // TODO: move to numass-processing with viewer feature

pub mod postprocess;
pub mod preprocess;
pub mod process;
pub mod script;
pub mod storage;
#[cfg(feature = "polars")]
pub mod to_polars;
#[cfg(feature = "polars")]
pub use to_polars::ToPolars;
#[cfg(feature = "python")]
mod python;
pub mod types;
pub mod utils;
#[cfg(feature = "egui")]
pub mod widgets;
#[cfg(feature = "stubgen")]
pub use python::stub_info;

mod constants;
mod query;

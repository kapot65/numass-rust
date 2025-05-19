use std::{path::PathBuf, net::SocketAddr, str::FromStr};

#[cfg(not(target_arch = "wasm32"))]
use clap::Parser;


#[cfg(not(target_arch = "wasm32"))]
#[derive(Parser, Debug, Clone)]
#[clap(author, version, about, long_about = None)]
pub struct Opt {
    pub directory: PathBuf,
    #[clap(long, default_value_t = SocketAddr::from_str("0.0.0.0:8085").unwrap())]
    pub address: SocketAddr
}

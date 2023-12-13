use apps::defaults::BOARD_IP;
use clap::Parser;

/// Perfroms a single acquisition and dumps some info about it to stdout
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// acquisition time in seconds
    #[arg(short, long, default_value_t = 1)]
    time: u32,

    #[arg(long, default_value_t = BOARD_IP)]
    tqdc_ip: std::net::IpAddr,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let args = Args::parse();

    let board = tqdc::TQDC::new(args.tqdc_ip);
    let events = board.acquire_point(args.time).await?;

    println!("{}", events.len());
    Ok(())
}

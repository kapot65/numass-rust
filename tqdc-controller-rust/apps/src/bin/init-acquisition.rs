use clap::Parser;
use apps::defaults::{
    HOST_IP, HOST_CONTROL_PORT, HOST_STREAM_PORT, 
    BOARD_IP, CONTROL_PORT, STREAM_PORT
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
   /// acquisition time in seconds
   #[arg(short, long, default_value_t = 1)]
   time: u32,

   #[arg(long, default_value_t = HOST_IP)]
   host_ip: std::net::IpAddr,

   #[arg(long, default_value_t = HOST_CONTROL_PORT)]
   host_control_port: u16,

   #[arg(long, default_value_t = HOST_STREAM_PORT)]
   host_stream_port: u16,

   #[arg(long, default_value_t = BOARD_IP)]
   tqdc_ip: std::net::IpAddr,

   #[arg(long, default_value_t = CONTROL_PORT)]
   tqdc_control_port: u16,

   #[arg(long, default_value_t = STREAM_PORT)]
   tqdc_stream_port: u16,
}

#[tokio::main]
async fn main() -> tokio::io::Result<()> {

    let args = Args::parse();

    let point = tqdc::acquire_point(
        args.time,
        args.host_ip,
        args.host_control_port,
        args.host_stream_port,
        args.tqdc_ip,
        args.tqdc_control_port,
        args.tqdc_stream_port
    ).await?;
    
    println!("{}", point.len());
    Ok(())
}
use chrono::{Local, NaiveDateTime};
use clap::Parser;

use std::io::Read;
use std::net::TcpStream;
use std::panic;
use std::path::PathBuf;
use std::time::Duration;
use ssh2::Session;


/// Watch power failure and detector last point timeout
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Detector SSH address
    #[arg(long, default_value = "192.168.111.140:22")]
    detector_ssh_addr: std::net::SocketAddr,
    
    /// Detector SSH user (user ssh key must be in detector allowed keys)
    #[arg(long, default_value = "chernov")]
    detector_ssh_user: String,
    
    /// Detector points backup path
    #[arg(long, default_value = "/data/tqdc-server-backup/")]
    detector_backup_folder: PathBuf,
    
    /// Maximum interval without new points (seconds)
    #[arg(long, default_value_t = 60)]
    interval: i64
}

fn infinite_alarm_sound(err: &str) -> ! {
    let failure_timestamp = Local::now().naive_local();
    println!("{failure_timestamp} {err}");
    
    loop {
        std::process::Command::new("speaker-test")
            .args(["-t", "sine", "-f", "1000", "-l", "1"])
            .output()
            .unwrap();
    }
}

fn main() {
    
    panic::set_hook(Box::new(|info| {
        infinite_alarm_sound(&info.to_string())
    }));
    
    let args = Args::parse();
    
    let last_point_hanler = std::thread::spawn(move || {
        let tcp = TcpStream::connect(args.detector_ssh_addr)
            .expect("failed to create TCP connection to detector");
        let mut sess = Session::new()
            .expect("failed to create ssh session");
        sess.set_tcp_stream(tcp);
        sess.handshake()
            .expect("failed to handshake ssh session");
        sess.userauth_agent(&args.detector_ssh_user)
            .expect("failed to set ssh useragent");
        
        loop {
            let mut channel = sess.channel_session()
                .expect("failed to start ssh session");
            channel.exec(&format!("ls -Art {} | tail -n 1", args.detector_backup_folder.to_str().unwrap()))
                .expect("failed to execute ls command");
            
            let last_point_timestamp ={
                let mut s = String::new();
                channel.read_to_string(&mut s)
                    .expect("failed to read string");
                NaiveDateTime::parse_from_str(&s[..19], "%Y-%m-%d-%H-%M-%S")
                    .expect("failed to parse point filename to time")
            };
            
            let delta_seconds = (Local::now().naive_local() - last_point_timestamp).num_seconds();    
            if  delta_seconds > args.interval {
                panic!("no new point in {} seconds", args.interval)
            }
            
            std::thread::sleep(Duration::from_secs(10))
        };
    });
    
    let battery_handler = std::thread::spawn(move || {
        
        let manager = battery::Manager::new().expect("failed to create battery manager");
        
        loop {
            let battery = manager.batteries()
                .expect("failed to list batteries")
                .next()
                .expect("failed get next battery")
                .expect("failed get first battery");
            
            if battery.state() == battery::State::Discharging {
                panic!("power failure detected");
            }
        }
    });
    
    last_point_hanler.join()
        .expect("failed to join last point thread");
    battery_handler.join()
        .expect("failed to join battery thread");
    
}

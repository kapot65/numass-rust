// default values used in CLI parsers
use std::net::{IpAddr, Ipv4Addr};

pub const HOST_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 4));
pub const HOST_CONTROL_PORT: u16 = 33330;
pub const HOST_STREAM_PORT: u16 = 33331;

pub const BOARD_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 8));
pub const CONTROL_PORT: u16 = 33300;
pub const STREAM_PORT: u16 = 33301;

use bincode::{Decode, Encode, config};
use clap::Parser;
use clap_num::number_range;
use ping::{PingResult, PingStats};
use socket2::{Domain, Protocol, Socket, Type};
use std::mem::{MaybeUninit, transmute};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use std::{process, thread};

fn package_count(s: &str) -> Result<u16, String> {
    number_range(s, 1, 4096)
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    host: String,
    #[arg(short, long, default_value = "0")]
    count: i16,
    #[arg(short, long, default_value = "1")]
    interval: u64,
    #[arg(short, long, default_value = "8", value_parser=package_count)]
    pc: u16,
    #[arg(short, long, default_value = "false")]
    fast: bool,
}
const ICMP_HEADER_SIZE: usize = 8;
const ICMP_ECHO_REQUEST: i8 = 8;
const ICMP_CODE: i8 = 0;
const MAX_DATA_SIZE: usize = 1600;

#[derive(Encode, Decode, Debug)]
struct MyPacket {
    _type: i8,     // b
    code: i8,      // b
    checksum: u16, // H
    id: u16,       // H
    seq: i16,      // h
} // 8

#[derive(Encode, Decode, Debug)]
struct DataPacket {
    data: f64,
}

fn get_timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

#[allow(clippy::cast_possible_truncation)]
fn checksum(source: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut count = 0;
    let max_count = (source.len() / 2) * 2;

    while count < max_count {
        let val = (u32::from(source[count + 1])) << 8 | (u32::from(source[count]));
        sum = sum.wrapping_add(val);
        count += 2;
    }

    if max_count < source.len() {
        sum = sum.wrapping_add(u32::from(source[source.len() - 1]));
    }

    sum = (sum >> 16) + (sum & 0xffff);
    sum += sum >> 16;
    !sum as u16
}

fn create_packet(id: u16, seq: i16, pc: u16) -> Vec<u8> {
    let mut header = MyPacket {
        _type: ICMP_ECHO_REQUEST,
        code: ICMP_CODE,
        checksum: 0,
        id,
        seq,
    };
    let data = DataPacket {
        data: get_timestamp(),
    };

    let cfg = config::standard().with_fixed_int_encoding();
    let header_buf: Vec<u8> = bincode::encode_to_vec(&header, cfg).unwrap();

    let data_buf: Vec<u8> = bincode::encode_to_vec(&data, cfg).unwrap();

    let mut combined_buf = Vec::with_capacity(header_buf.len() + data_buf.len());
    combined_buf.extend_from_slice(&header_buf);
    for _ in 0..pc {
        combined_buf.extend_from_slice(&data_buf);
    }

    let chksum = checksum(&combined_buf);
    header.checksum = chksum.to_be();

    let header_buf: Vec<u8> = bincode::encode_to_vec(&header, cfg).unwrap();
    let mut new_combined_buf = Vec::with_capacity(header_buf.len() + data_buf.len());
    new_combined_buf.extend_from_slice(&header_buf);
    for _ in 0..pc {
        new_combined_buf.extend_from_slice(&data_buf);
    }
    new_combined_buf
}

fn resolve_host(host: &str) -> std::io::Result<IpAddr> {
    use std::net::ToSocketAddrs;

    let socket = format!("{host}:0");
    let mut addrs = socket.to_socket_addrs()?;
    addrs
        .next()
        .map(|addr| addr.ip())
        .ok_or_else(|| std::io::Error::other("no IPs resolved"))
}

fn get_ips(host: &str) -> SocketAddr {
    let ip: Option<IpAddr> = host.parse().ok();
    let ip: IpAddr = match ip {
        Some(..) => ip.unwrap(),
        None => resolve_host(host).expect("ping: {host}: Name or service not known"),
    };
    let address: SocketAddr = SocketAddr::new(ip, 8080);
    address
}

#[allow(clippy::transmute_ptr_to_ptr)]
fn ping(address: SocketAddr, pid: u16, c: i16, pc: u16) -> PingResult {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4)).unwrap();
    let data: Vec<u8> = create_packet(pid, c, pc);
    let now = Instant::now();
    let mut ping_result = PingResult {
        transmitted: 0,
        received: 0,
        ping_delay: None,
    };
    let Ok(()) = socket.connect(&address.into()) else {
        return ping_result;
    };
    match socket.send(&data) {
        Ok(_) => ping_result.transmitted = 1,
        Err(_) => {
            return ping_result;
        }
    }

    let mut buffer = [0u8; MAX_DATA_SIZE];
    let len = {
        let buf: &mut [MaybeUninit<u8>; MAX_DATA_SIZE] = unsafe { transmute(&mut buffer) };
        socket.recv(buf)
    }
    .unwrap_or_default();
    if len < 1 {
        ping_result.ping_delay = Some(now.elapsed());
        return ping_result;
    }

    if buffer[ICMP_HEADER_SIZE..len] != data[ICMP_HEADER_SIZE..len] {
        ping_result.ping_delay = Some(now.elapsed());
        return ping_result;
    }
    ping_result.ping_delay = Some(now.elapsed());
    ping_result.received = 1;
    if let Some(delay) = ping_result.ping_delay {
        println!(
            "{} bytes from {} icmp_seq={} time={:.3} ms",
            len - ICMP_HEADER_SIZE,
            address.ip(),
            c,
            delay.as_secs_f64() * 1000.0,
        );
    }
    ping_result
}

#[allow(clippy::cast_possible_truncation)]
fn main() {
    let args = Args::parse();

    let pid: u16 = process::id() as u16;
    let address = if args.fast {
        println!("we are blazingly fast now {}", args.fast);
        SocketAddr::new(IpAddr::from_str("127.0.0.1").unwrap(), 8080)
    } else {
        get_ips(&args.host.clone())
    };
    let ping_interval = if args.fast { 0 } else { u64::max(args.interval, 1) };
    println!(
        "PING {} ({}) {} bytes of data.",
        args.host,
        address.ip(),
        16 * u64::from(args.pc)
    );
    let mut c = 1;

    let host = args.host.clone();
    let running = Arc::new(AtomicBool::new(true));
    let ping_stats = Arc::new(Mutex::new(PingStats::new()));
    // Setup Ctrl+C handler
    {
        let running = Arc::clone(&running);
        let ping_stats = Arc::clone(&ping_stats);
        ctrlc::set_handler(move || {
            println!("\nreceived Ctrl+C!");
            running.store(false, Ordering::SeqCst);
            ping_stats.lock().unwrap().finish();
            ping_stats.lock().unwrap().print_stat(&args.host.clone());
            process::exit(0);
        })
        .expect("Error setting Ctrl-C handler");
    }
    while args.count == 0 || c <= args.count {
        let ping_result = ping(address, pid, c, args.pc);
        ping_stats.lock().expect("damn").push(&ping_result);
        thread::sleep(Duration::from_secs(ping_interval));
        c += 1;
    }
    ping_stats.lock().unwrap().finish();
    ping_stats.lock().unwrap().print_stat(&host);
}

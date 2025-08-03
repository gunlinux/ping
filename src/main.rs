use bincode::{config, Decode, Encode};
use clap::Parser;
use clap_num::number_range;
use socket2::{Domain, Protocol, Socket, Type};
use std::mem::{MaybeUninit, transmute};
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};
use std::{process, thread};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

fn package_count(s: &str) -> Result<u16, String> {
    number_range(s, 1, 4096)
}

/// Simple program to greet a person

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    host: String,
    #[arg(short, long, default_value = "0")]
    count: i16,
    #[arg(short, long, default_value = "1")]
    interval: u64,
    #[arg(short, long, default_value = "8", value_parser=package_count)]
    pc: u16,
}
const ICMP_HEADER_SIZE: usize = 8;
const ICMP_ECHO_REQUEST: i8 = 8;
const ICMP_CODE: i8 = 0;

#[derive(Encode, Decode, Debug)]
struct MyPacket {
    _type: i8,     // b
    code: i8,      // b
    checksum: u16, // H
    id: u16,       // H
    _seq: i16,     // h
} // 8

#[derive(Debug, Clone)]
struct PingResult {
    transmitted: u16,
    received: u16,
    ping_delay: u128,
}

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

fn checksum(source: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut count = 0;
    let max_count = (source.len() / 2) * 2;

    while count < max_count {
        let val = (source[count + 1] as u32) << 8 | (source[count] as u32);
        sum = sum.wrapping_add(val);
        count += 2;
    }

    if max_count < source.len() {
        sum = sum.wrapping_add(source[source.len() - 1] as u32);
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
        id: id,
        _seq: seq,
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

fn get_ips(host: String) -> SocketAddr {
    let ip: Option<IpAddr> = host.parse().ok();
    let ip: IpAddr = match ip {
        Some(..) => ip.unwrap(),
        None => resolve_host(&host).expect("ping: {host}: Name or service not known"),
    };
    let address: SocketAddr = SocketAddr::new(ip, 8080);
    return address;
}

fn ping(address: SocketAddr, pid: u16, c: i16, pc: u16) -> Option<PingResult> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4)).unwrap();
    let data: Vec<u8> = create_packet(pid, c, pc);
    let now = Instant::now();
    let mut ping_result = PingResult {
        transmitted: 0,
        received: 0,
        ping_delay: 0,
    };
    let Ok(_) = socket.connect(&address.into()) else {
        return Some(ping_result);
    };
    match socket.send(&data) {
        Ok(_) => ping_result.transmitted = 1,
        Err(_) => {
            return Some(ping_result);
        }
    };

    let mut buffer = [0u8; u16::MAX as usize];
    let len = {
        let buf: &mut [MaybeUninit<u8>; u16::MAX as usize] = unsafe { transmute(&mut buffer) };
        socket.recv(buf)
    }
    .unwrap_or_default();
    if len < 1 {
        ping_result.ping_delay = now.elapsed().as_millis();
        return Some(ping_result);
    }

    if buffer[ICMP_HEADER_SIZE..len] != data[ICMP_HEADER_SIZE..len] {
        ping_result.ping_delay = now.elapsed().as_millis();
        return Some(ping_result);
    }
    ping_result.ping_delay = now.elapsed().as_millis();
    ping_result.received = 1;
    println!(
        "{} bytes from {} icmp_seq={} time={:?} ms",
        len - ICMP_HEADER_SIZE,
        address.ip(),
        c,
        ping_result.ping_delay,
    );
    Some(ping_result)
}

fn print_stat(ping_results: Vec<PingResult>, app_now: Instant, host: &str) {
    let mut final_result = PingResult {
        transmitted: 0,
        received: 0,
        ping_delay: 0,
    };
    let pcount = ping_results.len() as u64;
    if ping_results.is_empty() {
        println!("zero info");
        return
    }
    let mut min: u128 = ping_results[0].ping_delay;
    for i in &ping_results {
        final_result.transmitted += i.transmitted;
        final_result.received += i.received;
        final_result.ping_delay += i.ping_delay;
        if i.ping_delay < min {
            min = i.ping_delay;
        }
    }
    let avg = final_result.ping_delay as f64 / (ping_results.len() as f64);
    let success_percent: f64 = (final_result.received as f64 / ping_results.len() as f64) * 100.0;
    let loss = ((pcount - final_result.received as u64) as f64 / pcount as f64) * 100.0;
    let spend = app_now.elapsed().as_millis();
    println!("--- {} ping statistics ---", host);
    println!(
        "{} packets transmitted {} received, {}% packets loss, time {}sm",
        final_result.transmitted, final_result.received, loss, spend
    );
    println!("avg: {avg} / min: {min} / success % {success_percent}");
}

fn main() {
    // TODO Cli package Simple
    // cli package count
    let args = Args::parse();

    let pid: u16 = process::id() as u16;
    let address = get_ips(args.host.clone());
    let ping_interval = u64::max(args.interval, 1);
    let app_now = Instant::now();
    println!(
        "PING {} ({}) {} bytes of data.",
        args.host,
        address.ip(),
        16 * args.pc as u64
    );
    // let mut ping_results: Vec<PingResult> = vec![];
    let ping_results = Arc::new(Mutex::new(Vec::new()));
    let mut c = 1;

    let host = args.host.clone();
    let running = Arc::new(AtomicBool::new(true));
    // Setup Ctrl+C handler
    {
        let running = Arc::clone(&running);
        let ping_results = Arc::clone(&ping_results);
        ctrlc::set_handler(move || {
            println!("\nreceived Ctrl+C!");
            running.store(false, Ordering::SeqCst);
            //let results = print_stat(ping_results, app_now, host).lock().unwrap();
            print_stat(ping_results.lock().unwrap().to_vec(), app_now, &host);
            process::exit(0);
        })
        .expect("Error setting Ctrl-C handler");
    }
    while args.count == 0 || c <= args.count {
        let now = Instant::now();
        let ping_result = ping(address, pid, c, args.pc);
        let ping_result: PingResult = match ping_result {
            Some(_) => ping_result.unwrap(),
            None => PingResult {
                transmitted: 0,
                received: 0,
                ping_delay: now.elapsed().as_millis(),
            },
        };
        ping_results.lock().expect("dame").push(ping_result);
        thread::sleep(Duration::from_secs(ping_interval));
        c += 1;
    }
    print_stat(ping_results.lock().unwrap().to_vec(), app_now, &args.host.clone());
}

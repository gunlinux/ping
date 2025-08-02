use bincode::{Decode, Encode, config};
use clap::Parser;
use clap_num::number_range;
use socket2::{Domain, Protocol, Socket, Type};
use std::mem::{MaybeUninit, transmute};
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};
use std::{process, thread};

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
    sum as u16
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
        None => resolve_host(&host).unwrap(),
    };
    let address: SocketAddr = SocketAddr::new(ip, 8080);
    return address;
}

fn ping(address: SocketAddr, pid: u16, c: i16, pc: u16) {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4)).unwrap();
    let data: Vec<u8> = create_packet(pid, c, pc);
    let now = Instant::now();
    socket.connect(&address.into()).unwrap();
    socket.send(&data).unwrap();
    let mut buffer = [0u8; u16::MAX as usize];
    let len = {
        let buf: &mut [MaybeUninit<u8>; u16::MAX as usize] = unsafe { transmute(&mut buffer) };
        socket.recv(buf)
    }
    .unwrap();

    assert_eq!(&buffer[8..len], &data[8..len], "data failed");
    println!(
        "{} bytes from 1.1.1.1 icmp_seq={} time={:?} ms",
        len - 8,
        c,
        now.elapsed().as_millis(),
    );
}

fn main() {
    // TODO Cli package Simple
    // cli package count
    let args = Args::parse();

    let pid: u16 = process::id() as u16;
    let address = get_ips(args.host.clone());
    let ping_interval = u64::max(args.interval, 1);
    println!(
        "PING {} ({}) {} bytes of data.",
        args.host,
        address.ip(),
        16 * args.pc as u64
    );
    if args.count == 0 {
        let mut c = 1;
        loop {
            ping(address, pid, c, args.pc);
            thread::sleep(Duration::from_secs(ping_interval));
            c += 1;
        }
    }
    for c in 0..args.count {
        ping(address, pid, c, args.pc);
        thread::sleep(Duration::from_secs(ping_interval));
    }
}

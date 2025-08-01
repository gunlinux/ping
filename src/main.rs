use bincode::{Decode, Encode, config};
use socket2::{Domain, Protocol, Socket, Type};
use std::mem::{MaybeUninit, transmute};
use std::net::SocketAddr;
use std::process;
use std::time::{Duration, Instant};

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

    !(sum as u16)
}

fn create_packet(id: u16, _seq: i16) -> Vec<u8> {
    let mut header = MyPacket {
        _type: ICMP_ECHO_REQUEST,
        code: ICMP_CODE,
        checksum: 0,
        id: id,
        _seq: _seq,
    };
    let data = DataPacket {
        data: get_timestamp(),
    };

    let cfg = config::standard().with_fixed_int_encoding();
    let header_buf: Vec<u8> = bincode::encode_to_vec(&header, cfg).unwrap();

    let data_buf: Vec<u8> = bincode::encode_to_vec(&data, cfg).unwrap();

    let mut combined_buf = Vec::with_capacity(header_buf.len() + data_buf.len());
    combined_buf.extend_from_slice(&header_buf);
    combined_buf.extend_from_slice(&data_buf);

    let chksum = checksum(&combined_buf);
    header.checksum = chksum.to_be();
    println!("{}", chksum);

    let header_buf: Vec<u8> = bincode::encode_to_vec(&header, cfg).unwrap();
    let mut new_combined_buf = Vec::with_capacity(header_buf.len() + data_buf.len());
    println!(
        "cbughex: {}",
        combined_buf
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ")
    );

    new_combined_buf.extend_from_slice(&header_buf);
    new_combined_buf.extend_from_slice(&data_buf);
    return new_combined_buf;
}

fn main() {
    // TODO
    // validate returned package
    // повередение если пинг / резолв не сработал
    // cli интерфейф
    let pid: u16 = process::id() as u16;
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4)).unwrap();
    let address: SocketAddr = "1.1.1.1:666".parse().unwrap();
    println!("{}", address);

    let data: Vec<u8> = create_packet(pid, 1);
    println!("{:#?}", data);

    let now = Instant::now();
    socket.connect(&address.into()).unwrap();
    socket.send(&data).unwrap();
    let mut buffer = [0u8; 512];
    let len = {
        let buf: &mut [MaybeUninit<u8>; 512] = unsafe { transmute(&mut buffer) };
        socket.recv(buf)
    }
    .unwrap();

    println!("d is {:?}", now.elapsed());

    for i in &buffer[..len] {
        println!("{:#?}", i);
    }
}

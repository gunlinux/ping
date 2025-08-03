#![allow(dead_code)]

use std::time::{Instant};

#[derive(Debug, Clone)]
pub struct PingStats {
    count: u16,
    transmitted: u16,
    received: u16,
    ping_delay: u128,
    loss: u16,
    ping_min: u128,
    start: Instant,
}

#[derive(Debug, Clone)]
pub struct PingResult {
    pub transmitted: u16,
    pub received: u16,
    pub ping_delay: u128,
}

impl PingStats {
    #[must_use]
    pub fn new() -> Self {
        PingStats {
            count: 0,
            transmitted: 0,
            received: 0,
            ping_delay: 0,
            loss: 0,
            ping_min: u128::MAX,
            start: Instant::now(),
        }
    }
    pub fn push(&mut self, ping_result: PingResult) {
        self.count += 1;
        self.transmitted += ping_result.transmitted;
        self.received += ping_result.received;
        self.ping_delay += ping_result.ping_delay;
        if ping_result.received == 0 {
            self.loss += 1;
        }
        self.ping_min = u128::min(ping_result.ping_delay, self.ping_min);
    }
    pub fn print_stat(&self, host: &str) {
        println!("--- {host} ping statistics ---");
        println!(
            "{} packets transmitted {} received, {}% packets loss, time {}sm",
            self.transmitted,
            self.received,
            (self.loss / self.count) * 100,
            self.start.elapsed().as_millis()
        );
        println!("avg: {:.3}ms / min: {}/ success: {}%", self.avg(), self.ping_min, self.success());
    }

    fn avg(&self) -> f64 {
        self.ping_delay as f64 / self.count as f64
    }

    fn success(&self) -> f64 {
        (self.received as f64 / self.count as f64) * 100.0
    }
}

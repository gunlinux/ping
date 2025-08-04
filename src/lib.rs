#![allow(dead_code)]

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct PingStats {
    count: u16,
    transmitted: u16,
    received: u16,
    ping_delay: Option<Duration>,
    loss: u16,
    ping_min: Option<Duration>,
    start: Instant,
    duration: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct PingResult {
    pub transmitted: u16,
    pub received: u16,
    pub ping_delay: Option<Duration>,
}

impl Default for PingStats {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::cast_lossless, clippy::cast_precision_loss)]
impl PingStats {
    #[must_use]
    pub fn new() -> Self {
        PingStats {
            count: 0,
            transmitted: 0,
            received: 0,
            ping_delay: None,
            loss: 0,
            ping_min: None,
            start: Instant::now(),
            duration: None,
        }
    }
    pub fn push(&mut self, ping_result: &PingResult) {
        self.count += 1;
        self.transmitted += ping_result.transmitted;
        self.received += ping_result.received;

        self.ping_delay = match (self.ping_delay, ping_result.ping_delay) {
            (Some(a), Some(b)) => Some(a + b),
            (None, Some(b)) => Some(b),
            (Some(a), None) => Some(a),
            (None, None) => None,
        };
        self.ping_min = match (self.ping_min, self.ping_delay) {
            (Some(a), Some(b)) => Some(Duration::min(a, b)),
            (None, Some(b)) => Some(b),
            (None, None) => None,
            (Some(a), None) => Some(a),
        };
        if ping_result.received == 0 {
            self.loss += 1;
        }
    }
    pub fn finish(&mut self) {
        self.duration = Some(self.start.elapsed());
    }
    pub fn print_stat(&self, host: &str) {
        let finish_time = match self.duration {
            Some(a) => a.as_millis(),
            None => 0,
        };
        println!("--- {host} ping statistics ---");
        println!(
            "{} packets transmitted {} received, {}% packets loss, time {}sm",
            self.transmitted,
            self.received,
            (self.loss / self.count) * 100,
            finish_time,
        );
        println!(
            "avg: {:.3}ms / min: {:.3}/ success: {}%",
            self.avg(),
            self.get_ping_min(),
            self.success()
        );
    }

    fn avg(&self) -> f64 {
        match self.ping_delay {
            Some(a) => a.as_millis() as f64 / self.count as f64,
            None => 0.0,
        }
    }

    fn success(&self) -> f64 {
        (self.received as f64 / self.count as f64) * 100.0
    }
    fn get_ping_min(&self) -> f64 {
        match self.ping_min {
            Some(a) => a.as_millis() as f64,
            None => 0.0,
        }
    }
}

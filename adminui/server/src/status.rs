//! Live data straight from /proc (no DB): CPU, memory, uptime, per-interface
//! throughput history for the dashboard sparklines.

use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

pub const SAMPLE_EVERY: Duration = Duration::from_secs(2);
const KEEP: usize = 90; // 3 minutes of 2 s samples

#[derive(Default)]
pub struct Sampler {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    last_cpu: Option<(u64, u64)>, // (busy, total)
    cpu: VecDeque<f32>,
    last_dev: HashMap<String, (u64, u64)>,
    /// iface -> (rx B/s, tx B/s) history
    rates: HashMap<String, VecDeque<(f64, f64)>>,
    totals: HashMap<String, (u64, u64)>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Traffic {
    pub interval_secs: u64,
    pub cpu: Vec<f32>,
    pub ifaces: Vec<IfaceTraffic>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IfaceTraffic {
    pub name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_rate: Vec<f64>,
    pub tx_rate: Vec<f64>,
}

pub fn parse_net_dev(s: &str) -> Vec<(String, u64, u64)> {
    s.lines()
        .skip(2)
        .filter_map(|l| {
            let (name, rest) = l.split_once(':')?;
            let c: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            (c.len() >= 9).then(|| (name.trim().to_string(), c[0], c[8]))
        })
        .collect()
}

pub fn parse_cpu(s: &str) -> Option<(u64, u64)> {
    let l = s.lines().next()?;
    let v: Vec<u64> = l.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 4 {
        return None;
    }
    let total: u64 = v.iter().sum();
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    Some((total - idle, total))
}

impl Sampler {
    pub fn sample(&self) {
        let dev = std::fs::read_to_string("/proc/net/dev").unwrap_or_default();
        let stat = std::fs::read_to_string("/proc/stat").unwrap_or_default();
        let secs = SAMPLE_EVERY.as_secs_f64();
        let mut g = self.inner.lock().unwrap();
        if let Some((busy, total)) = parse_cpu(&stat) {
            if let Some((b0, t0)) = g.last_cpu {
                let dt = total.saturating_sub(t0).max(1);
                push(&mut g.cpu, busy.saturating_sub(b0) as f32 * 100.0 / dt as f32);
            }
            g.last_cpu = Some((busy, total));
        }
        for (name, rx, tx) in parse_net_dev(&dev) {
            if let Some((r0, t0)) = g.last_dev.get(&name).copied() {
                let r = rx.wrapping_sub(r0) as f64 / secs;
                let t = tx.wrapping_sub(t0) as f64 / secs;
                // 32-bit counters wrap; drop absurd samples.
                let sane = |x: f64| if x > 2.0e9 { 0.0 } else { x };
                push(g.rates.entry(name.clone()).or_default(), (sane(r), sane(t)));
            }
            g.last_dev.insert(name.clone(), (rx, tx));
            g.totals.insert(name, (rx, tx));
        }
    }

    pub fn traffic(&self) -> Traffic {
        let g = self.inner.lock().unwrap();
        let mut ifaces: Vec<IfaceTraffic> = g
            .rates
            .iter()
            .map(|(n, h)| {
                let (rx, tx) = g.totals.get(n).copied().unwrap_or_default();
                IfaceTraffic {
                    name: n.clone(),
                    rx_bytes: rx,
                    tx_bytes: tx,
                    rx_rate: h.iter().map(|x| x.0).collect(),
                    tx_rate: h.iter().map(|x| x.1).collect(),
                }
            })
            .collect();
        ifaces.sort_by(|a, b| a.name.cmp(&b.name));
        Traffic { interval_secs: SAMPLE_EVERY.as_secs(), cpu: g.cpu.iter().copied().collect(), ifaces }
    }
}

fn push<T>(q: &mut VecDeque<T>, v: T) {
    if q.len() >= KEEP {
        q.pop_front();
    }
    q.push_back(v);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct System {
    pub uptime_secs: u64,
    pub load: [f32; 3],
    pub mem_total_kb: u64,
    pub mem_free_kb: u64,
    pub mem_available_kb: u64,
    pub kernel: String,
    pub cpu_cores: usize,
}

pub fn system() -> System {
    let up = std::fs::read_to_string("/proc/uptime").unwrap_or_default();
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let mem = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let kv = |k: &str| {
        mem.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    let l: Vec<f32> = load.split_whitespace().take(3).filter_map(|x| x.parse().ok()).collect();
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let free = kv("MemFree:");
    let avail = match kv("MemAvailable:") {
        0 => free + kv("Cached:") + kv("Buffers:"),
        a => a,
    };
    System {
        uptime_secs: up.split('.').next().and_then(|x| x.parse().ok()).unwrap_or(0),
        load: [l.first().copied().unwrap_or(0.0), l.get(1).copied().unwrap_or(0.0), l.get(2).copied().unwrap_or(0.0)],
        mem_total_kb: kv("MemTotal:"),
        mem_free_kb: free,
        mem_available_kb: avail.min(kv("MemTotal:")),
        kernel: std::fs::read_to_string("/proc/version").unwrap_or_default().split_whitespace().take(3).collect::<Vec<_>>().join(" "),
        cpu_cores: cpuinfo.lines().filter(|l| l.starts_with("processor")).count(),
    }
}

#[derive(Serialize, Clone)]
pub struct ArpEntry {
    pub ip: String,
    pub mac: String,
    pub iface: String,
}

pub fn arp() -> Vec<ArpEntry> {
    std::fs::read_to_string("/proc/net/arp")
        .unwrap_or_default()
        .lines()
        .skip(1)
        .filter_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            (c.len() >= 6 && c[2] != "0x0").then(|| ArpEntry { ip: c[0].into(), mac: c[3].to_lowercase(), iface: c[5].into() })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_dev() {
        let s = "h1\nh2\n  br0: 54758092   60142    0  498    0     0          0         0  9907288   31641    0    0    0     0       0          0\n";
        assert_eq!(parse_net_dev(s), vec![("br0".to_string(), 54758092, 9907288)]);
    }

    #[test]
    fn cpu() {
        assert_eq!(parse_cpu("cpu  10 0 10 80 0 0 0\n"), Some((20, 100)));
    }
}

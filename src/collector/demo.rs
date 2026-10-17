//! Synthetic, deterministic data shaped like an Apple M3 Pro.
use std::time::Duration;

use super::Collector;
use crate::model::*;

pub struct DemoCollector {
    tick: u64,
    rng: u64,
}

const GB: u64 = 1024 * 1024 * 1024;
const MB: u64 = 1024 * 1024;

const PROC_NAMES: [&str; 32] = [
    "screencaptureui",
    "ControlCenter",
    "lazytop",
    "contactsd",
    "MenuBarAgent",
    "Claude",
    "suggestd",
    "WindowManager",
    "Browser Helper (Renderer)",
    "ollama",
    "kernel_task",
    "WindowServer",
    "launchd",
    "mds_stores",
    "Finder",
    "Dock",
    "zsh",
    "cargo",
    "rust-analyzer",
    "Terminal",
    "Slack Helper",
    "Music",
    "photoanalysisd",
    "coreaudiod",
    "bluetoothd",
    "cloudd",
    "nsurlsessiond",
    "trustd",
    "logd",
    "syslogd",
    "mDNSResponder",
    "Spotlight",
];

impl DemoCollector {
    pub fn new(seed: u64) -> Self {
        Self {
            tick: 0,
            rng: seed.wrapping_mul(6364136223846793005).wrapping_add(1),
        }
    }

    /// xorshift64*, returns 0.0..1.0
    fn rand(&mut self) -> f64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        (x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    fn wave(&mut self, base: f64, amp: f64, period: f64) -> f64 {
        let t = self.tick as f64;
        let noise = (self.rand() - 0.5) * amp * 0.6;
        (base + amp * (t / period * std::f64::consts::TAU).sin() + noise).clamp(0.0, 1.0)
    }
}

impl Collector for DemoCollector {
    fn sample(&mut self) -> Snapshot {
        self.tick += 1;
        let e = self.wave(0.15, 0.10, 23.0);
        let p = self.wave(0.05, 0.05, 37.0);
        let gpu = self.wave(0.10, 0.08, 17.0);
        let per_core: Vec<f64> = (0..11)
            .map(|i| {
                if i < 6 {
                    (e + (self.rand() - 0.5) * 0.1).clamp(0.0, 1.0)
                } else {
                    (p + (self.rand() - 0.5) * 0.05).clamp(0.0, 1.0)
                }
            })
            .collect();
        let total = per_core.iter().sum::<f64>() / per_core.len() as f64;
        let used = (10.8 * GB as f64 + self.rand() * 0.2 * GB as f64) as u64;
        let mut procs: Vec<ProcInfo> = PROC_NAMES
            .iter()
            .enumerate()
            .map(|(i, n)| ProcInfo {
                pid: 100 + i as u32 * 37,
                ppid: Some(if i == 0 { 1 } else { 100 }),
                name: (*n).into(),
                cmd: format!("/usr/bin/{n}"),
                user: "celson".into(),
                cpu: (self.rand() * 13.0) as f32,
                mem: (self.rand() * 500.0) as u64 * MB,
                threads: Some(1 + (self.rand() * 20.0) as usize),
                start_time: 1_760_000_000,
            })
            .collect();
        procs[0].ppid = None;
        let r = self.rand();
        Snapshot {
            host: HostInfo {
                chip: "Apple M3 Pro".into(),
                os: "macOS 27.0".into(),
                hostname: "demo".into(),
                cores_e: Some(6),
                cores_p: Some(5),
                cores_total: 11,
                uptime: Duration::from_secs(3 * 86_400 + 2 * 3600 + self.tick),
            },
            cpu: CpuStats {
                clusters: vec![
                    CpuCluster {
                        kind: ClusterKind::Efficiency,
                        usage: e,
                        mhz: Some(1400 + (e * 1000.0) as u32),
                    },
                    CpuCluster {
                        kind: ClusterKind::Performance,
                        usage: p,
                        mhz: Some(1200 + (p * 2800.0) as u32),
                    },
                ],
                per_core,
                total,
            },
            gpu: Some(GpuStats {
                name: "Apple M3 Pro GPU".into(),
                usage: gpu,
                mhz: Some(338 + (gpu * 1000.0) as u32),
                mem_used: Some(400 * MB),
                mem_total: None,
            }),
            power: Some(PowerStats {
                cpu_w: Some(1.0 + e * 5.0 + p * 10.0),
                gpu_w: Some(gpu * 2.0),
                ane_w: Some(0.0),
                dram_w: Some(0.4),
                system_w: Some(4.7 + r),
            }),
            mem: MemStats {
                total: 18 * GB,
                used,
                free: 18 * GB - used,
                wired: Some(18 * GB / 10),
                active: Some(61 * GB / 10),
                compressed: Some(29 * GB / 10),
                app: Some(7 * GB),
                cached: Some(52 * GB / 10),
                swap_used: 0,
                swap_total: 2 * GB,
                pressure: Some(0.26),
            },
            bandwidth: Some(Bandwidth {
                total: 13e9 * (0.8 + r * 0.4),
                cpu: 4e9,
                gpu: 6e9,
                media: 0.0,
                ane: 1e9,
                other: 2e9,
            }),
            net: NetStats {
                rx: 5_000.0 * (1.0 + r),
                tx: 3_000.0 * (1.0 + r),
                ifaces: vec![NetIface {
                    name: "en0".into(),
                    rx: 5_000.0 * (1.0 + r),
                    tx: 3_000.0 * (1.0 + r),
                }],
            },
            disk: DiskStats {
                read: Some(111_000.0 * (0.5 + r)),
                write: Some(40_000.0 * (0.5 + r)),
                used: 198 * GB,
                total: 494 * GB,
                read_total: Some(27 * GB),
                write_total: Some(8 * GB),
            },
            sensors: vec![
                SensorGroup {
                    kind: SensorKind::Cpu,
                    readings: vec![
                        ("Tp01".into(), 45.0 + r),
                        ("Tp05".into(), 44.0),
                        ("Tp09".into(), 46.0),
                    ],
                },
                SensorGroup {
                    kind: SensorKind::Gpu,
                    readings: (0..7)
                        .map(|i| (format!("Tg0{i}"), 41.0 + (i % 2) as f64))
                        .collect(),
                },
                SensorGroup {
                    kind: SensorKind::Battery,
                    readings: vec![
                        ("TB0T".into(), 32.0),
                        ("TB1T".into(), 33.0),
                        ("TB2T".into(), 32.0),
                    ],
                },
            ],
            fans: vec![],
            procs,
            ai: AiStats::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_same_seed() {
        let mut a = DemoCollector::new(7);
        let mut b = DemoCollector::new(7);
        assert_eq!(a.sample(), b.sample());
        assert_eq!(a.sample(), b.sample());
    }

    #[test]
    fn shape_is_complete() {
        let s = DemoCollector::new(1).sample();
        assert_eq!(s.cpu.clusters.len(), 2);
        assert_eq!(s.cpu.per_core.len(), 11);
        assert!(s.gpu.is_some() && s.power.is_some() && s.bandwidth.is_some());
        assert!(s.procs.len() >= 30);
        assert!(s.procs.iter().any(|p| p.name == "ollama"));
        assert!(s.sensors.iter().any(|g| g.kind == SensorKind::Cpu));
        for c in &s.cpu.clusters {
            assert!((0.0..=1.0).contains(&c.usage));
        }
        assert!(s.mem.used <= s.mem.total);
    }
}

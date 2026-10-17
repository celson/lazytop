use std::collections::HashMap;
use std::time::Instant;

use sysinfo::{Disks, Networks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users};

use super::rate::Counter;
use super::{Collector, Platform, PlatformData, ai};
use crate::model::*;

/// Refresh mounted-disk capacity every N samples.
const DISK_REFRESH_TICKS: u64 = 30;

pub fn merge(mut s: Snapshot, pd: PlatformData) -> Snapshot {
    if let Some(chip) = pd.chip {
        s.host.chip = chip;
    }
    s.host.cores_e = pd.cores_e;
    s.host.cores_p = pd.cores_p;
    s.cpu.clusters = if pd.clusters.is_empty() {
        vec![CpuCluster {
            kind: ClusterKind::All,
            usage: s.cpu.total,
            mhz: None,
        }]
    } else {
        pd.clusters
    };
    s.gpu = pd.gpu;
    s.power = pd.power;
    if let Some(m) = pd.mem {
        s.mem.used = m.used;
        s.mem.free = m.free;
        s.mem.wired = Some(m.wired);
        s.mem.active = Some(m.active);
        s.mem.compressed = Some(m.compressed);
        s.mem.app = Some(m.app);
        s.mem.cached = Some(m.cached);
        s.mem.pressure = m.pressure;
        s.mem.swap_used = m.swap_used;
        s.mem.swap_total = m.swap_total;
    }
    s.bandwidth = pd.bandwidth;
    s.sensors = pd.sensors;
    s.fans = pd.fans;
    s
}

/// sysinfo-based data shared by every OS.
pub struct Common {
    sys: System,
    nets: Networks,
    disks: Disks,
    users: Users,
    net_counters: HashMap<String, (Counter, Counter)>,
    ticks: u64,
}

impl Common {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        Self {
            sys,
            nets: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            users: Users::new_with_refreshed_list(),
            net_counters: HashMap::new(),
            ticks: 0,
        }
    }

    pub fn sample(&mut self) -> Snapshot {
        let now = Instant::now();
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        // Only what the UI shows; cmd/user are read once per process.
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_user(UpdateKind::OnlyIfNotSet)
                .with_cmd(UpdateKind::OnlyIfNotSet),
        );
        self.nets.refresh(true);
        // Disk capacity barely changes and statfs over every mount costs ~20 ms; refresh rarely.
        if self.ticks.is_multiple_of(DISK_REFRESH_TICKS) {
            self.disks.refresh(true);
        }
        self.ticks += 1;

        let per_core: Vec<f64> = self
            .sys
            .cpus()
            .iter()
            .map(|c| c.cpu_usage() as f64 / 100.0)
            .collect();
        let total = if per_core.is_empty() {
            0.0
        } else {
            per_core.iter().sum::<f64>() / per_core.len() as f64
        };

        let mut ifaces = Vec::new();
        for (name, data) in self.nets.iter() {
            if name.starts_with("lo")
                || name.starts_with("utun")
                || name.starts_with("awdl")
                || name.starts_with("llw")
            {
                continue;
            }
            let (rc, tc) = self.net_counters.entry(name.clone()).or_default();
            let rx = rc.update(data.total_received(), now).unwrap_or(0.0);
            let tx = tc.update(data.total_transmitted(), now).unwrap_or(0.0);
            if data.total_received() > 0 || data.total_transmitted() > 0 {
                ifaces.push(NetIface {
                    name: name.clone(),
                    rx,
                    tx,
                });
            }
        }
        ifaces.sort_by(|a, b| (b.rx + b.tx).total_cmp(&(a.rx + a.tx)));
        let net = NetStats {
            rx: ifaces.iter().map(|i| i.rx).sum(),
            tx: ifaces.iter().map(|i| i.tx).sum(),
            ifaces,
        };

        // The root filesystem (or the largest one) represents "Used".
        let disk = self
            .disks
            .iter()
            .find(|d| d.mount_point() == std::path::Path::new("/System/Volumes/Data"))
            .or_else(|| {
                self.disks
                    .iter()
                    .find(|d| d.mount_point() == std::path::Path::new("/"))
            })
            .map(|d| DiskStats {
                used: d.total_space() - d.available_space(),
                total: d.total_space(),
                ..Default::default()
            })
            .unwrap_or_default();

        let procs = self
            .sys
            .processes()
            .values()
            .map(|p| ProcInfo {
                pid: p.pid().as_u32(),
                ppid: p.parent().map(|pp| pp.as_u32()),
                name: p.name().to_string_lossy().into_owned(),
                cmd: p
                    .cmd()
                    .iter()
                    .map(|c| c.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
                user: p
                    .user_id()
                    .and_then(|u| self.users.get_user_by_id(u))
                    .map(|u| u.name().to_string())
                    .unwrap_or_default(),
                cpu: p.cpu_usage(),
                mem: p.memory(),
                threads: p.tasks().map(|t| t.len()),
                start_time: p.start_time(),
            })
            .collect();

        let total_mem = self.sys.total_memory();
        let used_mem = self.sys.used_memory();
        Snapshot {
            host: HostInfo {
                chip: self
                    .sys
                    .cpus()
                    .first()
                    .map(|c| c.brand().trim().to_string())
                    .unwrap_or_default(),
                os: System::long_os_version().unwrap_or_default(),
                hostname: System::host_name().unwrap_or_default(),
                cores_e: None,
                cores_p: None,
                cores_total: per_core.len(),
                uptime: std::time::Duration::from_secs(System::uptime()),
            },
            cpu: CpuStats {
                clusters: vec![],
                per_core,
                total,
            },
            mem: MemStats {
                total: total_mem,
                used: used_mem,
                free: total_mem.saturating_sub(used_mem),
                swap_used: self.sys.used_swap(),
                swap_total: self.sys.total_swap(),
                ..Default::default()
            },
            net,
            disk,
            procs,
            ..Default::default()
        }
    }
}

impl Default for Common {
    fn default() -> Self {
        Self::new()
    }
}

/// Common + platform backend.
pub struct SystemCollector {
    common: Common,
    platform: Box<dyn Platform>,
    disk_r: Counter,
    disk_w: Counter,
}

impl SystemCollector {
    pub fn new(platform: Box<dyn Platform>) -> Self {
        Self {
            common: Common::new(),
            platform,
            disk_r: Counter::new(),
            disk_w: Counter::new(),
        }
    }
}

impl Collector for SystemCollector {
    fn sample(&mut self) -> Snapshot {
        let now = Instant::now();
        let base = self.common.sample();
        self.platform.set_per_core(&base.cpu.per_core);
        let pd = self.platform.sample();
        let totals = pd.disk_totals;
        let mut s = merge(base, pd);
        if let Some((r, w)) = totals {
            s.disk.read = self.disk_r.update(r, now);
            s.disk.write = self.disk_w.update(w, now);
            s.disk.read_total = Some(r);
            s.disk.write_total = Some(w);
        }
        s.ai = ai::analyze(&s);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::MemDetail;

    fn base() -> Snapshot {
        Snapshot {
            host: HostInfo {
                chip: "generic".into(),
                cores_total: 4,
                ..Default::default()
            },
            cpu: CpuStats {
                per_core: vec![0.2, 0.4, 0.6, 0.8],
                total: 0.5,
                clusters: vec![],
            },
            mem: MemStats {
                total: 100,
                used: 60,
                free: 40,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn merge_without_platform_derives_all_cluster() {
        let s = merge(base(), PlatformData::default());
        assert_eq!(
            s.cpu.clusters,
            vec![CpuCluster {
                kind: ClusterKind::All,
                usage: 0.5,
                mhz: None
            }]
        );
        assert_eq!(s.host.chip, "generic");
        assert!(s.gpu.is_none());
    }

    #[test]
    fn merge_with_platform_overrides() {
        let pd = PlatformData {
            chip: Some("Apple M3 Pro".into()),
            cores_e: Some(2),
            cores_p: Some(2),
            clusters: vec![CpuCluster {
                kind: ClusterKind::Efficiency,
                usage: 0.1,
                mhz: Some(1000),
            }],
            mem: Some(MemDetail {
                used: 70,
                free: 30,
                wired: 10,
                active: 20,
                compressed: 5,
                app: 50,
                cached: 15,
                pressure: Some(0.3),
                swap_used: 1,
                swap_total: 2,
            }),
            ..Default::default()
        };
        let s = merge(base(), pd);
        assert_eq!(s.host.chip, "Apple M3 Pro");
        assert_eq!(s.cpu.clusters.len(), 1);
        assert_eq!(s.mem.used, 70);
        assert_eq!(s.mem.wired, Some(10));
        assert_eq!(s.mem.pressure, Some(0.3));
        assert_eq!(s.mem.swap_total, 2);
    }
    #[test]
    fn common_samples_real_machine() {
        let mut c = Common::new();
        let _ = c.sample();
        std::thread::sleep(std::time::Duration::from_millis(250));
        let s = c.sample();
        assert!(s.mem.total > 0);
        assert!(!s.cpu.per_core.is_empty());
        assert!(s.procs.iter().any(|p| p.pid == std::process::id()));
    }
}

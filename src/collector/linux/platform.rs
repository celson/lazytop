use std::path::PathBuf;
use std::time::Instant;

use super::{amdgpu, diskstats, hwmon, rapl, topology};
use crate::collector::{Platform, PlatformData};
use crate::model::*;

pub fn rapl_watts(
    prev: &mut Vec<(u64, Instant)>,
    zones: &[rapl::Zone],
    now: Instant,
) -> Option<PowerStats> {
    let cur: Vec<Option<u64>> = zones.iter().map(rapl::read_uj).collect();
    let mut out = PowerStats::default();
    let mut any = false;
    if prev.len() == zones.len() {
        for ((z, c), (pv, pt)) in zones.iter().zip(&cur).zip(prev.iter()) {
            let Some(c) = c else { continue };
            let dt = now.saturating_duration_since(*pt).as_secs_f64();
            if dt <= 0.0 {
                continue;
            }
            let w = rapl::delta_uj(*pv, *c, z.max_uj) as f64 / 1e6 / dt;
            let slot = if z.name.starts_with("package") {
                &mut out.cpu_w
            } else if z.name == "psys" {
                &mut out.system_w
            } else if z.name == "dram" {
                &mut out.dram_w
            } else {
                continue;
            };
            *slot = Some(slot.unwrap_or(0.0) + w);
            any = true;
        }
    }
    *prev = cur.into_iter().map(|c| (c.unwrap_or(0), now)).collect();
    any.then_some(out)
}

pub struct LinuxPlatform {
    root: PathBuf,
    hybrid: Option<(Vec<usize>, Vec<usize>)>,
    zones: Vec<rapl::Zone>,
    rapl_prev: Vec<(u64, Instant)>,
    per_core: Vec<f64>,
    #[cfg(target_os = "linux")]
    nvml: Option<super::nvml::NvmlGpu>,
}

impl LinuxPlatform {
    pub fn new(root: PathBuf) -> Self {
        let zones = rapl::zones(&root);
        if zones.is_empty() {
            crate::log::warn_once(
                "rapl",
                "RAPL energy counters not readable; power shown as —",
            );
        }
        Self {
            hybrid: topology::hybrid(&root),
            zones,
            rapl_prev: Vec::new(),
            per_core: Vec::new(),
            #[cfg(target_os = "linux")]
            nvml: super::nvml::NvmlGpu::new(),
            root,
        }
    }

    fn cluster(&self, kind: ClusterKind, cpus: &[usize]) -> CpuCluster {
        let usages: Vec<f64> = cpus
            .iter()
            .filter_map(|i| self.per_core.get(*i).copied())
            .collect();
        let usage = if usages.is_empty() {
            0.0
        } else {
            usages.iter().sum::<f64>() / usages.len() as f64
        };
        let freqs: Vec<u32> = cpus
            .iter()
            .filter_map(|i| topology::cur_freq_mhz(&self.root, *i))
            .collect();
        let mhz = (!freqs.is_empty()).then(|| freqs.iter().sum::<u32>() / freqs.len() as u32);
        CpuCluster { kind, usage, mhz }
    }
}

impl Platform for LinuxPlatform {
    fn set_per_core(&mut self, per_core: &[f64]) {
        self.per_core = per_core.to_vec();
    }

    fn sample(&mut self) -> PlatformData {
        let now = Instant::now();
        let (sensors, fans) = hwmon::read(&self.root);
        let mut pd = PlatformData {
            sensors,
            fans,
            disk_totals: diskstats::totals(&self.root),
            ..Default::default()
        };
        match &self.hybrid {
            Some((p, e)) => {
                pd.cores_p = Some(p.len());
                pd.cores_e = Some(e.len());
                pd.clusters = vec![
                    self.cluster(ClusterKind::Efficiency, e),
                    self.cluster(ClusterKind::Performance, p),
                ];
            }
            None => {
                let all: Vec<usize> = (0..self.per_core.len()).collect();
                if !all.is_empty() {
                    pd.clusters = vec![self.cluster(ClusterKind::All, &all)];
                }
            }
        }
        pd.power = rapl_watts(&mut self.rapl_prev, &self.zones, now);

        #[cfg(target_os = "linux")]
        if let Some(n) = &self.nvml {
            match n.read() {
                Ok((g, w)) => {
                    pd.gpu = Some(g);
                    if let Some(w) = w {
                        pd.power.get_or_insert_with(Default::default).gpu_w = Some(w);
                    }
                }
                Err(e) => crate::log::warn_once("nvml", &e),
            }
        }
        if pd.gpu.is_none()
            && let Some((g, w)) = amdgpu::read(&self.root)
        {
            pd.gpu = Some(g);
            if let Some(w) = w {
                pd.power.get_or_insert_with(Default::default).gpu_w = Some(w);
            }
        }
        pd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::linux::testfs::put;
    use std::time::Duration;

    #[test]
    fn platform_from_fake_root() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        put(r, "sys/devices/cpu_core/cpus", "0-1");
        put(r, "sys/devices/cpu_atom/cpus", "2-3");
        put(
            r,
            "sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
            "3000000",
        );
        put(
            r,
            "sys/devices/system/cpu/cpu1/cpufreq/scaling_cur_freq",
            "1000000",
        );
        put(r, "sys/class/hwmon/hwmon0/name", "coretemp");
        put(r, "sys/class/hwmon/hwmon0/temp1_input", "50000");
        put(r, "proc/diskstats", "8 0 sda 1 0 2 0 1 0 4 0 0 0 0\n");
        put(r, "sys/block/sda/stat", "");
        let mut p = LinuxPlatform::new(r.to_path_buf());
        p.set_per_core(&[0.5, 0.7, 0.1, 0.3]);
        let pd = p.sample();
        assert_eq!(pd.cores_p, Some(2));
        assert_eq!(pd.cores_e, Some(2));
        let perf = pd
            .clusters
            .iter()
            .find(|c| c.kind == ClusterKind::Performance)
            .unwrap();
        assert!((perf.usage - 0.6).abs() < 1e-9);
        assert_eq!(perf.mhz, Some(2000));
        assert_eq!(pd.sensors.len(), 1);
        assert_eq!(pd.disk_totals, Some((1024, 2048)));
        assert!(pd.gpu.is_none());
        assert!(pd.power.is_none());
    }

    #[test]
    fn rapl_watts_needs_two_samples() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        put(r, "sys/class/powercap/intel-rapl:0/name", "package-0");
        put(r, "sys/class/powercap/intel-rapl:0/energy_uj", "1000000");
        put(
            r,
            "sys/class/powercap/intel-rapl:0/max_energy_range_uj",
            "100000000",
        );
        let zones = rapl::zones(r);
        let mut prev = Vec::new();
        let t0 = Instant::now();
        assert_eq!(rapl_watts(&mut prev, &zones, t0), None);
        put(r, "sys/class/powercap/intel-rapl:0/energy_uj", "3000000");
        let p = rapl_watts(&mut prev, &zones, t0 + Duration::from_secs(1)).unwrap();
        assert_eq!(p.cpu_w, Some(2.0));
    }
}

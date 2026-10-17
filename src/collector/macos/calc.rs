//! Pure math behind the macOS backend; no FFI here so it is tested on every OS.
use crate::collector::MemDetail;
use crate::model::{Bandwidth, PowerStats, SensorKind};

pub const IDLE_STATES: [&str; 3] = ["IDLE", "OFF", "DOWN"];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VmCounts {
    pub free: u64,
    pub active: u64,
    pub inactive: u64,
    pub wired: u64,
    pub speculative: u64,
    pub purgeable: u64,
    pub compressor: u64,
    pub internal: u64,
    pub external: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Cpu,
    Gpu,
    Media,
    Ane,
    Other,
}

/// Usage (0..1) and residency-weighted MHz from IOReport state residencies.
/// Active states map in order onto `freqs_mhz`; extra states reuse the last frequency.
pub fn residency_usage_mhz(states: &[(String, i64)], freqs_mhz: &[u32]) -> (f64, Option<u32>) {
    let total: i64 = states.iter().map(|s| s.1.max(0)).sum();
    if total <= 0 {
        return (0.0, None);
    }
    let active: Vec<i64> = states
        .iter()
        .filter(|(n, _)| !IDLE_STATES.contains(&n.as_str()))
        .map(|s| s.1.max(0))
        .collect();
    let active_sum: i64 = active.iter().sum();
    let usage = active_sum as f64 / total as f64;
    if active_sum == 0 || freqs_mhz.is_empty() {
        return (usage, None);
    }
    let weighted: f64 = active
        .iter()
        .enumerate()
        .map(|(i, r)| *r as f64 * freqs_mhz[i.min(freqs_mhz.len() - 1)] as f64)
        .sum();
    (usage, Some((weighted / active_sum as f64).round() as u32))
}

pub fn watts_from_energy(value: i64, unit: &str, dt_secs: f64) -> Option<f64> {
    if dt_secs <= 0.0 {
        return None;
    }
    let joules_per_unit = match unit.trim() {
        "mJ" => 1e-3,
        "uJ" | "µJ" => 1e-6,
        "nJ" => 1e-9,
        _ => return None,
    };
    Some(value.max(0) as f64 * joules_per_unit / dt_secs)
}

/// IORegistry `voltage-states*` blobs: pairs of LE u32 (freq, voltage).
pub fn parse_freq_table(data: &[u8]) -> Vec<u32> {
    let freqs: Vec<u64> = data
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as u64)
        .filter(|f| *f > 0)
        .collect();
    let is_hz = freqs.iter().any(|f| *f > 10_000_000);
    freqs
        .into_iter()
        .map(|f| {
            if is_hz {
                (f / 1_000_000) as u32
            } else {
                (f / 1_000) as u32
            }
        })
        .collect()
}

pub fn classify_smc_key(key: &str) -> Option<SensorKind> {
    let mut c = key.chars();
    if c.next()? != 'T' {
        return None;
    }
    Some(match c.next()? {
        'p' | 'e' | 'C' => SensorKind::Cpu,
        'g' | 'G' => SensorKind::Gpu,
        'B' => SensorKind::Battery,
        'H' => SensorKind::Disk,
        _ => SensorKind::Other,
    })
}

/// Activity Monitor style breakdown. `pressure_level` is `kern.memorystatus_level` (% free).
pub fn mem_detail(
    vm: &VmCounts,
    page_size: u64,
    total: u64,
    pressure_level: Option<u32>,
    swap: (u64, u64),
) -> MemDetail {
    let app = vm.internal.saturating_sub(vm.purgeable) * page_size;
    let wired = vm.wired * page_size;
    let compressed = vm.compressor * page_size;
    let used = (app + wired + compressed).min(total);
    MemDetail {
        used,
        free: total - used,
        wired,
        active: vm.active * page_size,
        compressed,
        app,
        cached: (vm.external + vm.purgeable) * page_size,
        pressure: pressure_level.map(|l| (100u32.saturating_sub(l.min(100))) as f64 / 100.0),
        swap_used: swap.0,
        swap_total: swap.1,
    }
}

/// Mean of a residency histogram whose state names are bin values like `"  12GB/s"` / `" 500MB/s"`.
/// Returns GB/s; `None` when no residency or no parsable bin.
pub fn histogram_mean_gbps(states: &[(String, i64)]) -> Option<f64> {
    let (mut weighted, mut total) = (0.0, 0i64);
    for (name, res) in states {
        let n = name.trim();
        let gb = if let Some(v) = n.strip_suffix("GB/s") {
            v.trim().parse::<f64>().ok()
        } else if let Some(v) = n.strip_suffix("MB/s") {
            v.trim().parse::<f64>().ok().map(|m| m / 1000.0)
        } else {
            None
        };
        let (Some(gb), r) = (gb, (*res).max(0)) else {
            continue;
        };
        weighted += gb * r as f64;
        total += r;
    }
    (total > 0).then(|| weighted / total as f64)
}

/// On some macOS releases the mJ CPU/DRAM energy counters stop updating: a zero CPU delta while
/// the CPU is busy means "unavailable", not 0 W, and DRAM never really draws 0 W. The system total
/// is only the sum of parts when the CPU part is known; otherwise it stays `None` (shown as —).
pub fn finalize_power(mut p: PowerStats, cpu_busy: bool) -> PowerStats {
    if cpu_busy && p.cpu_w == Some(0.0) {
        p.cpu_w = None;
    }
    if p.dram_w == Some(0.0) {
        p.dram_w = None;
    }
    p.system_w = p
        .cpu_w
        .map(|cpu| cpu + [p.gpu_w, p.ane_w, p.dram_w].iter().flatten().sum::<f64>());
    p
}

/// Per-agent DCS bandwidth (bytes/s) → buckets. Total is the sum of agents (same convention as
/// SiliconScope); the aggregate `"RD+WR"` channel is ignored.
pub fn bandwidth_from_agents(agents: &[(String, f64)]) -> Bandwidth {
    let mut bw = Bandwidth::default();
    for (name, bps) in agents {
        if name == "RD+WR" {
            continue;
        }
        let slot = match bandwidth_bucket(name) {
            Bucket::Cpu => &mut bw.cpu,
            Bucket::Gpu => &mut bw.gpu,
            Bucket::Media => &mut bw.media,
            Bucket::Ane => &mut bw.ane,
            Bucket::Other => &mut bw.other,
        };
        *slot += bps;
    }
    bw.total = bw.cpu + bw.gpu + bw.media + bw.ane + bw.other;
    bw
}

pub fn bandwidth_bucket(channel: &str) -> Bucket {
    let c = channel.to_uppercase();
    if c.contains("CPU") || c.starts_with("EACC") || c.starts_with("PACC") {
        Bucket::Cpu
    } else if c.starts_with("GFX") || c.starts_with("GPU") || c.starts_with("AGX") {
        Bucket::Gpu
    } else if c.starts_with("ANE") {
        Bucket::Ane
    } else if [
        "AVE", "AVD", "VDEC", "VENC", "ISP", "MSR", "JPG", "JPEG", "SCODEC", "PRORES", "MEDIA",
    ]
    .iter()
    .any(|p| c.starts_with(p))
    {
        Bucket::Media
    } else {
        Bucket::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(v: &[(&str, i64)]) -> Vec<(String, i64)> {
        v.iter().map(|(n, r)| (n.to_string(), *r)).collect()
    }

    #[test]
    fn residency_math() {
        // 50% idle; active split 1:3 between 1000 and 2000 MHz → 1750
        let s = st(&[("IDLE", 400), ("V0P5", 100), ("V1P4", 300)]);
        let (u, mhz) = residency_usage_mhz(&s, &[1000, 2000]);
        assert!((u - 0.5).abs() < 1e-9);
        assert_eq!(mhz, Some(1750));
    }

    #[test]
    fn residency_all_idle_or_empty() {
        assert_eq!(
            residency_usage_mhz(&st(&[("IDLE", 10), ("OFF", 5)]), &[1000]),
            (0.0, None)
        );
        assert_eq!(residency_usage_mhz(&[], &[]), (0.0, None));
    }

    #[test]
    fn residency_more_states_than_freqs_uses_last_freq() {
        let s = st(&[("OFF", 0), ("A", 1), ("B", 1)]);
        assert_eq!(residency_usage_mhz(&s, &[600]), (1.0, Some(600)));
        // no freq table → usage only
        assert_eq!(residency_usage_mhz(&s, &[]), (1.0, None));
    }

    #[test]
    fn energy_units() {
        assert_eq!(watts_from_energy(2_000, "mJ", 1.0), Some(2.0));
        assert_eq!(watts_from_energy(2_000_000, "uJ", 2.0), Some(1.0));
        assert_eq!(watts_from_energy(500_000_000, "nJ", 0.5), Some(1.0));
        assert_eq!(watts_from_energy(5, "parsecs", 1.0), None);
        assert_eq!(watts_from_energy(5, "mJ", 0.0), None);
        assert_eq!(watts_from_energy(-5, "mJ", 1.0), Some(0.0));
    }

    #[test]
    fn freq_table_hz_and_khz() {
        let mut hz = Vec::new();
        for (f, v) in [(0u32, 0u32), (600_000_000, 800), (1_200_000_000, 900)] {
            hz.extend_from_slice(&f.to_le_bytes());
            hz.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(parse_freq_table(&hz), vec![600, 1200]);
        let mut khz = Vec::new();
        for (f, v) in [(912_000u32, 800u32), (4_512_000, 900)] {
            khz.extend_from_slice(&f.to_le_bytes());
            khz.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(parse_freq_table(&khz), vec![912, 4512]);
        assert_eq!(parse_freq_table(&[1, 2, 3]), Vec::<u32>::new());
    }

    #[test]
    fn smc_key_classes() {
        assert_eq!(classify_smc_key("Tp01"), Some(SensorKind::Cpu));
        assert_eq!(classify_smc_key("Te05"), Some(SensorKind::Cpu));
        assert_eq!(classify_smc_key("Tg0f"), Some(SensorKind::Gpu));
        assert_eq!(classify_smc_key("TB1T"), Some(SensorKind::Battery));
        assert_eq!(classify_smc_key("TH0x"), Some(SensorKind::Disk));
        assert_eq!(classify_smc_key("Ts0P"), Some(SensorKind::Other));
        assert_eq!(classify_smc_key("F0Ac"), None);
    }

    #[test]
    fn mem_detail_activity_monitor_formula() {
        let vm = VmCounts {
            free: 100,
            active: 400,
            inactive: 200,
            wired: 100,
            speculative: 50,
            purgeable: 20,
            compressor: 150,
            internal: 450,
            external: 180,
        };
        let ps = 16_384;
        let total = 1_000 * ps;
        let m = mem_detail(&vm, ps, total, Some(74), (1, 2));
        assert_eq!(m.app, (450 - 20) * ps);
        assert_eq!(m.wired, 100 * ps);
        assert_eq!(m.compressed, 150 * ps);
        assert_eq!(m.used, (430 + 100 + 150) * ps);
        assert_eq!(m.cached, (180 + 20) * ps);
        assert_eq!(m.free, total - m.used);
        assert_eq!(m.pressure, Some(0.26));
        assert_eq!((m.swap_used, m.swap_total), (1, 2));
    }

    #[test]
    fn histogram_mean_from_bin_labels() {
        let s = st(&[("   2GB/s", 3), ("   4GB/s", 1), ("  64GB/s", 0)]);
        assert_eq!(histogram_mean_gbps(&s), Some(2.5));
        assert_eq!(histogram_mean_gbps(&st(&[("   2GB/s", 0)])), None);
        assert_eq!(histogram_mean_gbps(&st(&[("garbage", 5)])), None);
        assert_eq!(histogram_mean_gbps(&st(&[(" 500MB/s", 1)])), Some(0.5));
    }

    #[test]
    fn frozen_cpu_energy_makes_cpu_and_system_unknown() {
        let p = PowerStats {
            cpu_w: Some(0.0),
            gpu_w: Some(0.05),
            ane_w: Some(0.0),
            dram_w: Some(0.0),
            system_w: None,
        };
        let f = finalize_power(p, true);
        assert_eq!((f.cpu_w, f.dram_w, f.system_w), (None, None, None));
        assert_eq!((f.gpu_w, f.ane_w), (Some(0.05), Some(0.0)));
    }

    #[test]
    fn working_cpu_energy_sums_system() {
        let p = PowerStats {
            cpu_w: Some(2.0),
            gpu_w: Some(0.5),
            ane_w: Some(0.0),
            dram_w: Some(0.5),
            system_w: None,
        };
        assert_eq!(finalize_power(p, true).system_w, Some(3.0));
        // idle CPU reporting 0 W is trusted when the CPU is idle
        let idle = PowerStats {
            cpu_w: Some(0.0),
            gpu_w: Some(0.0),
            ..Default::default()
        };
        assert_eq!(finalize_power(idle, false).cpu_w, Some(0.0));
    }

    #[test]
    fn bandwidth_total_is_sum_of_agents() {
        let agents = vec![
            ("EACC0 RD+WR".to_string(), 2e9),
            ("PACC0 RD+WR".to_string(), 2e9),
            ("AGX RD+WR".to_string(), 6e9),
            ("ANE0 RD+WR".to_string(), 1e9),
            ("DISP RD+WR".to_string(), 2e9),
            ("RD+WR".to_string(), 4e9), // aggregate channel is ignored
        ];
        let b = bandwidth_from_agents(&agents);
        assert_eq!(
            (b.cpu, b.gpu, b.ane, b.media, b.other),
            (4e9, 6e9, 1e9, 0.0, 2e9)
        );
        assert_eq!(b.total, 13e9);
    }

    #[test]
    fn dcs_agent_buckets() {
        assert_eq!(bandwidth_bucket("EACC0 RD+WR"), Bucket::Cpu);
        assert_eq!(bandwidth_bucket("PACC0 RD+WR"), Bucket::Cpu);
        assert_eq!(bandwidth_bucket("AGX RD+WR"), Bucket::Gpu);
        assert_eq!(bandwidth_bucket("AVD RD+WR"), Bucket::Media);
        assert_eq!(bandwidth_bucket("JPEG RD+WR"), Bucket::Media);
        assert_eq!(bandwidth_bucket("SCODEC RD+WR"), Bucket::Media);
        assert_eq!(bandwidth_bucket("DISPEXT0 RD+WR"), Bucket::Other);
    }

    #[test]
    fn bandwidth_buckets() {
        assert_eq!(bandwidth_bucket("PCPU0 RD+WR"), Bucket::Cpu);
        assert_eq!(bandwidth_bucket("ECPU RD"), Bucket::Cpu);
        assert_eq!(bandwidth_bucket("GFX RD+WR"), Bucket::Gpu);
        assert_eq!(bandwidth_bucket("GPU0"), Bucket::Gpu);
        assert_eq!(bandwidth_bucket("ANE0 RD"), Bucket::Ane);
        assert_eq!(bandwidth_bucket("AVE RD"), Bucket::Media);
        assert_eq!(bandwidth_bucket("VDEC WR"), Bucket::Media);
        assert_eq!(bandwidth_bucket("ISP RD"), Bucket::Media);
        assert_eq!(bandwidth_bucket("DISP0"), Bucket::Other);
    }
}

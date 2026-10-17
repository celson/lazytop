use std::path::Path;

fn read_trim(p: &Path) -> Option<String> {
    std::fs::read_to_string(p)
        .ok()
        .map(|s| s.trim().to_string())
}

pub fn parse_cpu_list(s: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for part in s.trim().split(',').filter(|p| !p.is_empty()) {
        match part.split_once('-') {
            Some((a, b)) => {
                if let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>()) {
                    out.extend(a..=b);
                }
            }
            None => {
                if let Ok(n) = part.parse() {
                    out.push(n);
                }
            }
        }
    }
    out
}

/// (performance cores, efficiency cores) on Intel hybrid CPUs.
pub fn hybrid(root: &Path) -> Option<(Vec<usize>, Vec<usize>)> {
    let p = parse_cpu_list(&read_trim(&root.join("sys/devices/cpu_core/cpus"))?);
    let e = parse_cpu_list(&read_trim(&root.join("sys/devices/cpu_atom/cpus"))?);
    (!p.is_empty() && !e.is_empty()).then_some((p, e))
}

pub fn cur_freq_mhz(root: &Path, cpu: usize) -> Option<u32> {
    let s = read_trim(&root.join(format!(
        "sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_cur_freq"
    )))?;
    s.parse::<u64>().ok().map(|khz| (khz / 1000) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::linux::testfs::put;

    #[test]
    fn cpu_list() {
        assert_eq!(parse_cpu_list("0-3,8,10-11\n"), vec![0, 1, 2, 3, 8, 10, 11]);
        assert_eq!(parse_cpu_list(""), Vec::<usize>::new());
        assert_eq!(parse_cpu_list("garbage"), Vec::<usize>::new());
    }

    #[test]
    fn hybrid_and_freq() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(hybrid(d.path()), None);
        put(d.path(), "sys/devices/cpu_core/cpus", "0-3");
        put(d.path(), "sys/devices/cpu_atom/cpus", "4-7");
        assert_eq!(hybrid(d.path()), Some((vec![0, 1, 2, 3], vec![4, 5, 6, 7])));
        put(
            d.path(),
            "sys/devices/system/cpu/cpu2/cpufreq/scaling_cur_freq",
            "2400000\n",
        );
        assert_eq!(cur_freq_mhz(d.path(), 2), Some(2400));
        assert_eq!(cur_freq_mhz(d.path(), 3), None);
    }
}

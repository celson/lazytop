use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    pub name: String,
    pub path: PathBuf,
    pub max_uj: u64,
}

fn read_u64(p: &Path) -> Option<u64> {
    std::fs::read_to_string(p).ok()?.trim().parse().ok()
}

/// Energy consumed between two readings of a counter that wraps at `max`.
pub fn delta_uj(prev: u64, cur: u64, max: u64) -> u64 {
    if cur >= prev {
        cur - prev
    } else if max > prev {
        (max - prev) + cur
    } else {
        0
    }
}

/// Top-level zones (`intel-rapl:N`, not subzones) whose counter is readable by this user.
pub fn zones(root: &Path) -> Vec<Zone> {
    let Ok(entries) = std::fs::read_dir(root.join("sys/class/powercap")) else {
        return Vec::new();
    };
    let mut out: Vec<Zone> = entries
        .flatten()
        .filter_map(|e| {
            let fname = e.file_name().into_string().ok()?;
            let rest = fname.strip_prefix("intel-rapl:")?;
            if rest.contains(':') {
                return None;
            }
            let path = e.path();
            let name = std::fs::read_to_string(path.join("name"))
                .ok()?
                .trim()
                .to_string();
            let zone = Zone {
                name,
                max_uj: read_u64(&path.join("max_energy_range_uj")).unwrap_or(0),
                path,
            };
            read_uj(&zone).map(|_| zone)
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

pub fn read_uj(z: &Zone) -> Option<u64> {
    read_u64(&z.path.join("energy_uj"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::linux::testfs::put;

    #[test]
    fn rapl_wraps() {
        assert_eq!(delta_uj(100, 300, 1000), 200);
        assert_eq!(delta_uj(900, 100, 1000), 200);
        assert_eq!(delta_uj(900, 100, 0), 0);
    }

    #[test]
    fn finds_top_level_zones() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        put(r, "sys/class/powercap/intel-rapl:0/name", "package-0\n");
        put(r, "sys/class/powercap/intel-rapl:0/energy_uj", "123\n");
        put(
            r,
            "sys/class/powercap/intel-rapl:0/max_energy_range_uj",
            "262143328850\n",
        );
        put(r, "sys/class/powercap/intel-rapl:0:0/name", "core\n");
        put(r, "sys/class/powercap/intel-rapl:0:0/energy_uj", "1\n");
        let z = zones(r);
        assert_eq!(z.len(), 1);
        assert_eq!(z[0].name, "package-0");
        assert_eq!(z[0].max_uj, 262143328850);
        assert_eq!(read_uj(&z[0]), Some(123));
    }
}

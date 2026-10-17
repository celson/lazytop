use std::collections::BTreeMap;
use std::path::Path;

use crate::model::{Fan, SensorGroup, SensorKind};

pub fn classify(name: &str) -> SensorKind {
    let n = name.to_lowercase();
    if ["coretemp", "k10temp", "zenpower", "cpu_thermal", "cpu"]
        .iter()
        .any(|p| n.starts_with(p))
    {
        SensorKind::Cpu
    } else if ["amdgpu", "nouveau", "nvidia", "radeon", "i915", "xe"]
        .iter()
        .any(|p| n.starts_with(p))
    {
        SensorKind::Gpu
    } else if n.starts_with("bat") {
        SensorKind::Battery
    } else if n.starts_with("nvme") || n.starts_with("drivetemp") {
        SensorKind::Disk
    } else {
        SensorKind::Other
    }
}

fn read_num(p: &Path) -> Option<f64> {
    std::fs::read_to_string(p).ok()?.trim().parse().ok()
}

pub fn read(root: &Path) -> (Vec<SensorGroup>, Vec<Fan>) {
    let mut groups: BTreeMap<SensorKind, Vec<(String, f64)>> = BTreeMap::new();
    let mut fans = Vec::new();
    let Ok(entries) = std::fs::read_dir(root.join("sys/class/hwmon")) else {
        return (Vec::new(), fans);
    };
    let mut dirs: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    dirs.sort();
    for dir in dirs {
        let name = std::fs::read_to_string(dir.join("name"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let kind = classify(&name);
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<String> = files
            .flatten()
            .filter_map(|f| f.file_name().into_string().ok())
            .collect();
        files.sort();
        for f in files {
            if let Some(idx) = f
                .strip_prefix("temp")
                .and_then(|r| r.strip_suffix("_input"))
            {
                let Some(milli) = read_num(&dir.join(&f)) else {
                    continue;
                };
                let c = milli / 1000.0;
                if !(-40.0..=150.0).contains(&c) {
                    continue;
                }
                let label = std::fs::read_to_string(dir.join(format!("temp{idx}_label")))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|_| format!("{name} temp{idx}"));
                groups.entry(kind).or_default().push((label, c));
            } else if let Some(idx) = f.strip_prefix("fan").and_then(|r| r.strip_suffix("_input"))
                && let Some(rpm) = read_num(&dir.join(&f))
            {
                fans.push(Fan {
                    label: format!("{name} fan{idx}"),
                    rpm,
                });
            }
        }
    }
    let groups = groups
        .into_iter()
        .map(|(kind, readings)| SensorGroup { kind, readings })
        .collect();
    (groups, fans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::linux::testfs::put;

    #[test]
    fn groups_by_driver() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        put(r, "sys/class/hwmon/hwmon0/name", "coretemp\n");
        put(r, "sys/class/hwmon/hwmon0/temp1_input", "45000\n");
        put(r, "sys/class/hwmon/hwmon0/temp1_label", "Package id 0\n");
        put(r, "sys/class/hwmon/hwmon0/temp2_input", "47000\n");
        put(r, "sys/class/hwmon/hwmon1/name", "nvme\n");
        put(r, "sys/class/hwmon/hwmon1/temp1_input", "38850\n");
        put(r, "sys/class/hwmon/hwmon2/name", "thinkpad\n");
        put(r, "sys/class/hwmon/hwmon2/fan1_input", "2100\n");
        put(r, "sys/class/hwmon/hwmon2/temp1_input", "-273000\n"); // bogus, dropped
        let (groups, fans) = read(r);
        let cpu = groups.iter().find(|g| g.kind == SensorKind::Cpu).unwrap();
        assert_eq!(
            cpu.readings,
            vec![
                ("Package id 0".to_string(), 45.0),
                ("coretemp temp2".to_string(), 47.0)
            ]
        );
        assert!(groups.iter().any(|g| g.kind == SensorKind::Disk));
        assert!(!groups.iter().any(|g| g.kind == SensorKind::Other));
        assert_eq!(
            fans,
            vec![Fan {
                label: "thinkpad fan1".into(),
                rpm: 2100.0
            }]
        );
    }

    #[test]
    fn classify_names() {
        assert_eq!(classify("k10temp"), SensorKind::Cpu);
        assert_eq!(classify("amdgpu"), SensorKind::Gpu);
        assert_eq!(classify("BAT0"), SensorKind::Battery);
        assert_eq!(classify("acpitz"), SensorKind::Other);
    }

    #[test]
    fn missing_root_is_empty() {
        let (g, f) = read(Path::new("/definitely/not/here"));
        assert!(g.is_empty() && f.is_empty());
    }
}

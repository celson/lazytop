use std::path::Path;

use crate::model::GpuStats;

fn read_trim(p: &Path) -> Option<String> {
    std::fs::read_to_string(p)
        .ok()
        .map(|s| s.trim().to_string())
}

/// First AMD card: (stats, watts).
pub fn read(root: &Path) -> Option<(GpuStats, Option<f64>)> {
    let mut cards: Vec<_> = std::fs::read_dir(root.join("sys/class/drm"))
        .ok()?
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with("card") && !n.contains('-')
        })
        .map(|e| e.path())
        .collect();
    cards.sort();
    let card = cards
        .into_iter()
        .find(|c| read_trim(&c.join("device/vendor")).as_deref() == Some("0x1002"))?;
    let dev = card.join("device");
    let usage = read_trim(&dev.join("gpu_busy_percent"))?
        .parse::<f64>()
        .ok()?
        / 100.0;
    let mhz = read_trim(&dev.join("pp_dpm_sclk")).and_then(|s| {
        s.lines()
            .find(|l| l.ends_with('*'))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| {
                v.trim_end_matches("Mhz")
                    .trim_end_matches("MHz")
                    .parse()
                    .ok()
            })
    });
    let watts = std::fs::read_dir(dev.join("hwmon"))
        .ok()
        .and_then(|mut it| {
            let h = it.next()?.ok()?.path();
            let uw: f64 = read_trim(&h.join("power1_average"))
                .or_else(|| read_trim(&h.join("power1_input")))?
                .parse()
                .ok()?;
            Some(uw / 1e6)
        });
    let name = format!("AMD GPU ({})", card.file_name()?.to_string_lossy());
    Some((
        GpuStats {
            name,
            usage,
            mhz,
            mem_used: read_trim(&dev.join("mem_info_vram_used")).and_then(|s| s.parse().ok()),
            mem_total: read_trim(&dev.join("mem_info_vram_total")).and_then(|s| s.parse().ok()),
        },
        watts,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::linux::testfs::put;

    #[test]
    fn reads_first_amd_card() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        put(r, "sys/class/drm/card0/device/vendor", "0x8086\n");
        put(r, "sys/class/drm/card1/device/vendor", "0x1002\n");
        put(r, "sys/class/drm/card1/device/gpu_busy_percent", "37\n");
        put(
            r,
            "sys/class/drm/card1/device/mem_info_vram_used",
            "1073741824\n",
        );
        put(
            r,
            "sys/class/drm/card1/device/mem_info_vram_total",
            "8589934592\n",
        );
        put(
            r,
            "sys/class/drm/card1/device/pp_dpm_sclk",
            "0: 500Mhz\n1: 1800Mhz *\n2: 2400Mhz\n",
        );
        put(
            r,
            "sys/class/drm/card1/device/hwmon/hwmon3/power1_average",
            "45000000\n",
        );
        let (g, w) = read(r).unwrap();
        assert!((g.usage - 0.37).abs() < 1e-9);
        assert_eq!(g.mhz, Some(1800));
        assert_eq!(g.mem_used, Some(1 << 30));
        assert_eq!(g.mem_total, Some(8 << 30));
        assert_eq!(w, Some(45.0));
        assert_eq!(g.name, "AMD GPU (card1)");
    }

    #[test]
    fn none_without_amd() {
        let d = tempfile::tempdir().unwrap();
        put(d.path(), "sys/class/drm/card0/device/vendor", "0x10de\n");
        assert!(read(d.path()).is_none());
    }
}

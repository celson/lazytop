use std::path::Path;

/// Cumulative (bytes read, bytes written) over whole block devices (no partitions, loop, ram, zram).
pub fn totals(root: &Path) -> Option<(u64, u64)> {
    let s = std::fs::read_to_string(root.join("proc/diskstats")).ok()?;
    let (mut r, mut w) = (0u64, 0u64);
    for line in s.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 {
            continue;
        }
        let name = f[2];
        if ["loop", "ram", "zram", "dm-", "md"]
            .iter()
            .any(|p| name.starts_with(p))
        {
            continue;
        }
        if !root.join("sys/block").join(name).exists() {
            continue; // partition
        }
        r += f[5].parse::<u64>().unwrap_or(0) * 512;
        w += f[9].parse::<u64>().unwrap_or(0) * 512;
    }
    Some((r, w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::linux::testfs::put;

    #[test]
    fn sums_whole_disks_only() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        put(
            r,
            "proc/diskstats",
            "   7       0 loop0 10 0 100 0 0 0 0 0 0 0 0\n\
             259       0 nvme0n1 500 0 2000 0 600 0 4000 0 0 0 0\n\
             259       1 nvme0n1p1 400 0 1000 0 300 0 2000 0 0 0 0\n\
               8       0 sda 1 0 10 0 1 0 20 0 0 0 0\n",
        );
        put(r, "sys/block/nvme0n1/stat", "");
        put(r, "sys/block/sda/stat", "");
        put(r, "sys/block/loop0/stat", "");
        // sectors are 512 bytes; loop devices are ignored
        assert_eq!(totals(r), Some(((2000 + 10) * 512, (4000 + 20) * 512)));
    }

    #[test]
    fn missing_is_none() {
        assert_eq!(totals(Path::new("/nope")), None);
    }
}

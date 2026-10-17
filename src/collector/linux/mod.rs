pub mod amdgpu;
pub mod diskstats;
pub mod hwmon;
pub mod rapl;
pub mod topology;

#[cfg(test)]
pub(crate) mod testfs {
    use std::path::Path;

    /// Writes `content` to `root/rel`, creating parent dirs.
    pub fn put(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
}

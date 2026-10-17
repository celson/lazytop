pub mod calc;
#[cfg(target_os = "macos")]
pub mod cf;
#[cfg(target_os = "macos")]
pub mod iokit;
#[cfg(target_os = "macos")]
pub mod ioreport;
#[cfg(target_os = "macos")]
pub mod smc;
#[cfg(target_os = "macos")]
pub mod sys;

#[cfg(target_os = "macos")]
pub use platform::MacPlatform;

#[cfg(target_os = "macos")]
mod platform {
    use super::{calc, iokit, ioreport::IoReport, smc::Smc, sys};
    use crate::collector::{Platform, PlatformData};
    use crate::model::GpuStats;

    pub struct MacPlatform {
        chip: Option<String>,
        cores_e: Option<usize>,
        cores_p: Option<usize>,
        page_size: u64,
        total: u64,
        ioreport: Option<IoReport>,
        smc: Option<Smc>,
    }

    impl MacPlatform {
        pub fn new() -> Self {
            let ioreport = IoReport::new()
                .map_err(|e| crate::log::warn_once("ioreport", &e.to_string()))
                .ok();
            if let Some(r) = &ioreport {
                r.log_channels();
            }
            let smc = Smc::open()
                .map_err(|e| crate::log::warn_once("smc", &e.to_string()))
                .ok();
            Self {
                chip: sys::sysctl_string("machdep.cpu.brand_string"),
                // perflevel0 = performance, perflevel1 = efficiency on Apple Silicon
                cores_p: sys::sysctl_u32("hw.perflevel0.logicalcpu").map(|v| v as usize),
                cores_e: sys::sysctl_u32("hw.perflevel1.logicalcpu").map(|v| v as usize),
                page_size: sys::sysctl_u64("hw.pagesize").unwrap_or(16_384),
                total: sys::sysctl_u64("hw.memsize").unwrap_or(0),
                ioreport,
                smc,
            }
        }
    }

    impl Default for MacPlatform {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Platform for MacPlatform {
        fn sample(&mut self) -> PlatformData {
            let mut pd = PlatformData {
                chip: self.chip.clone(),
                cores_e: self.cores_e,
                cores_p: self.cores_p,
                disk_totals: iokit::disk_totals(),
                ..Default::default()
            };
            if let Some(vm) = sys::vm_counts() {
                let level = sys::sysctl_u32("kern.memorystatus_level");
                let swap = sys::swap().unwrap_or((0, 0));
                pd.mem = Some(calc::mem_detail(
                    &vm,
                    self.page_size,
                    self.total,
                    level,
                    swap,
                ));
            } else {
                crate::log::warn_once("vm", "host_statistics64 failed");
            }
            if let Some(r) = &mut self.ioreport {
                match r.sample() {
                    Ok(s) => {
                        pd.clusters = s.clusters;
                        if let Some(u) = s.gpu_usage {
                            pd.gpu = Some(GpuStats {
                                name: format!("{} GPU", self.chip.clone().unwrap_or_default()),
                                usage: u,
                                mhz: s.gpu_mhz,
                                mem_used: iokit::gpu_mem_used(),
                                mem_total: None,
                            });
                        }
                        if s.power.system_w.is_some() {
                            pd.power = Some(s.power);
                        }
                        pd.bandwidth = s.bandwidth;
                    }
                    Err(e) => crate::log::warn_once("ioreport-sample", &e.to_string()),
                }
            }
            if let Some(smc) = &mut self.smc {
                pd.sensors = smc.temperatures();
                pd.fans = smc.fans();
                if let Some(w) = smc.system_power() {
                    pd.power.get_or_insert_with(Default::default).system_w = Some(w);
                }
            }
            pd
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn smoke_mac_platform() {
            let mut p = MacPlatform::new();
            let _ = p.sample();
            std::thread::sleep(std::time::Duration::from_millis(300));
            let pd = p.sample();
            assert!(pd.chip.is_some());
            assert!(pd.mem.is_some());
            if std::env::consts::ARCH == "aarch64" {
                assert_eq!(pd.cores_e.unwrap() + pd.cores_p.unwrap(), num_cores());
                assert_eq!(pd.clusters.len(), 2, "{:?}", pd.clusters);
                assert!(pd.gpu.is_some() && pd.power.is_some());
                // whole-system power from SMC PSTR, not just the GPU/ANE IOReport sum
                let sys_w = pd.power.as_ref().unwrap().system_w.unwrap();
                assert!(sys_w > 0.5, "{sys_w}");
            }
        }

        fn num_cores() -> usize {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(0)
        }
    }
}

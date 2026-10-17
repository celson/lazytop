use nvml_wrapper::Nvml;
use nvml_wrapper::enum_wrappers::device::Clock;

use crate::model::GpuStats;

/// NVIDIA GPU 0 via NVML; `libnvidia-ml.so` is loaded at runtime, absence → `None`.
pub struct NvmlGpu {
    nvml: Nvml,
}

impl NvmlGpu {
    pub fn new() -> Option<Self> {
        match Nvml::init() {
            Ok(nvml) if nvml.device_count().unwrap_or(0) > 0 => Some(Self { nvml }),
            Ok(_) => None,
            Err(e) => {
                crate::log::warn_once("nvml-init", &format!("NVML unavailable: {e}"));
                None
            }
        }
    }

    pub fn read(&self) -> Result<(GpuStats, Option<f64>), String> {
        let d = self.nvml.device_by_index(0).map_err(|e| e.to_string())?;
        let util = d.utilization_rates().map_err(|e| e.to_string())?;
        let mem = d.memory_info().ok();
        Ok((
            GpuStats {
                name: d.name().unwrap_or_else(|_| "NVIDIA GPU".into()),
                usage: util.gpu as f64 / 100.0,
                mhz: d.clock_info(Clock::Graphics).ok(),
                mem_used: mem.as_ref().map(|m| m.used),
                mem_total: mem.as_ref().map(|m| m.total),
            },
            d.power_usage().ok().map(|mw| mw as f64 / 1000.0),
        ))
    }
}

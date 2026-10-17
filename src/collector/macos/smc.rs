use std::collections::BTreeMap;

use anyhow::bail;

use super::calc;
use super::iokit::{
    self, IOConnectCallStructMethod, IOServiceClose, IOServiceOpen, IoObject, mach_task_self_,
};
use crate::model::{Fan, SensorGroup, SensorKind};

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct KeyDataVer {
    major: u8,
    minor: u8,
    build: u8,
    reserved: u8,
    release: u16,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct PLimitData {
    version: u16,
    length: u16,
    cpu_p_limit: u32,
    gpu_p_limit: u32,
    mem_p_limit: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct KeyInfo {
    data_size: u32,
    data_type: u32,
    data_attributes: u8,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct KeyData {
    key: u32,
    vers: KeyDataVer,
    p_limit_data: PLimitData,
    key_info: KeyInfo,
    result: u8,
    status: u8,
    data8: u8,
    data32: u32,
    bytes: [u8; 32],
}

const CMD_READ_BYTES: u8 = 5;
const CMD_READ_INDEX: u8 = 8;
const CMD_READ_KEYINFO: u8 = 9;
const KERNEL_INDEX_SMC: u32 = 2;

fn fourcc(s: &str) -> u32 {
    s.bytes().fold(0u32, |acc, b| (acc << 8) | b as u32)
}

fn fourcc_str(v: u32) -> String {
    String::from_utf8_lossy(&v.to_be_bytes()).into_owned()
}

pub struct Smc {
    conn: IoObject,
    /// Discovered temperature keys with their cached key info (saves one IOKit call per read).
    temp_keys: Vec<(String, u32, KeyInfo, SensorKind)>,
    fan_count: u8,
}

unsafe impl Send for Smc {}

impl Drop for Smc {
    fn drop(&mut self) {
        unsafe { IOServiceClose(self.conn) };
    }
}

impl Smc {
    pub fn open() -> anyhow::Result<Self> {
        let svc = iokit::services("AppleSMC").into_iter().next();
        let Some(svc) = svc else {
            bail!("AppleSMC service not found")
        };
        let mut conn: IoObject = 0;
        let kr = unsafe { IOServiceOpen(svc.0, mach_task_self_, 0, &mut conn) };
        if kr != 0 {
            bail!("IOServiceOpen(AppleSMC) failed: {kr}");
        }
        let mut smc = Self {
            conn,
            temp_keys: Vec::new(),
            fan_count: 0,
        };
        smc.discover();
        Ok(smc)
    }

    fn call(&self, input: &KeyData) -> Option<KeyData> {
        let mut out = KeyData::default();
        let mut size = std::mem::size_of::<KeyData>();
        let kr = unsafe {
            IOConnectCallStructMethod(
                self.conn,
                KERNEL_INDEX_SMC,
                input as *const _ as *const _,
                std::mem::size_of::<KeyData>(),
                &mut out as *mut _ as *mut _,
                &mut size,
            )
        };
        (kr == 0 && out.result == 0).then_some(out)
    }

    fn key_info(&self, key: u32) -> Option<KeyInfo> {
        self.call(&KeyData {
            key,
            data8: CMD_READ_KEYINFO,
            ..Default::default()
        })
        .map(|o| o.key_info)
    }

    fn read(&self, key: &str) -> Option<(KeyInfo, [u8; 32])> {
        let k = fourcc(key);
        let info = self.key_info(k)?;
        let out = self.call(&KeyData {
            key: k,
            key_info: info,
            data8: CMD_READ_BYTES,
            ..Default::default()
        })?;
        Some((info, out.bytes))
    }

    fn read_f32(&self, key: &str) -> Option<f32> {
        let (info, b) = self.read(key)?;
        Self::decode_f32(&info, &b)
    }

    fn decode_f32(info: &KeyInfo, b: &[u8; 32]) -> Option<f32> {
        (info.data_type == fourcc("flt ") && info.data_size == 4)
            .then(|| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_f32_cached(&self, key: u32, info: KeyInfo) -> Option<f32> {
        let out = self.call(&KeyData {
            key,
            key_info: info,
            data8: CMD_READ_BYTES,
            ..Default::default()
        })?;
        Self::decode_f32(&info, &out.bytes)
    }

    fn discover(&mut self) {
        let count = self
            .read("#KEY")
            .map(|(_, b)| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            .unwrap_or(0);
        for i in 0..count {
            let Some(out) = self.call(&KeyData {
                data8: CMD_READ_INDEX,
                data32: i,
                ..Default::default()
            }) else {
                continue;
            };
            let name = fourcc_str(out.key);
            // "Other" (skin, board, ...) has >100 keys on Apple Silicon and isn't shown; skip it.
            let Some(kind) = calc::classify_smc_key(&name).filter(|k| *k != SensorKind::Other)
            else {
                continue;
            };
            let Some(info) = self.key_info(out.key) else {
                continue;
            };
            if let Some(v) = self.read_f32_cached(out.key, info)
                && (5.0..=130.0).contains(&v)
            {
                self.temp_keys.push((name, out.key, info, kind));
            }
        }
        self.fan_count = self.read("FNum").map(|(_, b)| b[0]).unwrap_or(0);
    }

    pub fn temperatures(&mut self) -> Vec<SensorGroup> {
        let mut groups: BTreeMap<SensorKind, Vec<(String, f64)>> = BTreeMap::new();
        for (name, key, info, kind) in &self.temp_keys {
            if let Some(v) = self.read_f32_cached(*key, *info)
                && (5.0..=130.0).contains(&v)
            {
                groups
                    .entry(*kind)
                    .or_default()
                    .push((name.clone(), v as f64));
            }
        }
        groups
            .into_iter()
            .map(|(kind, readings)| SensorGroup { kind, readings })
            .collect()
    }

    /// Total system power in watts (SMC `PSTR`), when the machine exposes it.
    pub fn system_power(&mut self) -> Option<f64> {
        self.read_f32("PSTR")
            .filter(|w| w.is_finite() && *w >= 0.0)
            .map(|w| w as f64)
    }

    pub fn fans(&mut self) -> Vec<Fan> {
        (0..self.fan_count)
            .filter_map(|i| {
                self.read_f32(&format!("F{i}Ac")).map(|rpm| Fan {
                    label: format!("Fan {i}"),
                    rpm: rpm as f64,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keydata_layout_is_80_bytes() {
        assert_eq!(std::mem::size_of::<KeyData>(), 80);
    }

    #[test]
    fn smoke_reads_temperatures() {
        if !crate::collector::macos::hw_smoke() {
            return;
        }
        let mut smc = Smc::open().unwrap();
        let t = smc.temperatures();
        assert!(t.iter().any(|g| g.kind == SensorKind::Cpu), "{t:?}");
    }

    #[test]
    fn smoke_system_power() {
        if !crate::collector::macos::hw_smoke() {
            return;
        }
        let mut smc = Smc::open().unwrap();
        let w = smc.system_power().expect("PSTR");
        assert!((0.1..300.0).contains(&w), "{w}");
    }
}

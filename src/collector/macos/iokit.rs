use std::ffi::{CStr, CString, c_char};

use core_foundation::base::{CFAllocatorRef, CFTypeRef, kCFAllocatorDefault};
use core_foundation::dictionary::{CFDictionaryRef, CFMutableDictionaryRef};

use super::calc;
use super::cf::{self, Owned};

pub type IoObject = u32;
pub type KernReturn = i32;
pub type MachPort = u32;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    pub static kIOMainPortDefault: MachPort;
    pub fn IOServiceMatching(name: *const c_char) -> CFMutableDictionaryRef;
    pub fn IOServiceGetMatchingServices(
        main: MachPort,
        matching: CFDictionaryRef,
        it: *mut IoObject,
    ) -> KernReturn;
    pub fn IOIteratorNext(it: IoObject) -> IoObject;
    pub fn IOObjectRelease(o: IoObject) -> KernReturn;
    pub fn IORegistryEntryGetName(e: IoObject, name: *mut c_char) -> KernReturn;
    pub fn IORegistryEntryCreateCFProperties(
        e: IoObject,
        props: *mut CFMutableDictionaryRef,
        alloc: CFAllocatorRef,
        opts: u32,
    ) -> KernReturn;
    pub fn IOServiceOpen(
        svc: IoObject,
        task: MachPort,
        kind: u32,
        conn: *mut IoObject,
    ) -> KernReturn;
    pub fn IOServiceClose(conn: IoObject) -> KernReturn;
    pub fn IOConnectCallStructMethod(
        conn: IoObject,
        selector: u32,
        input: *const std::ffi::c_void,
        input_size: usize,
        output: *mut std::ffi::c_void,
        output_size: *mut usize,
    ) -> KernReturn;
}

unsafe extern "C" {
    pub static mach_task_self_: MachPort;
}

pub struct Service(pub IoObject);

impl Drop for Service {
    fn drop(&mut self) {
        unsafe { IOObjectRelease(self.0) };
    }
}

impl Service {
    pub fn name(&self) -> Option<String> {
        let mut buf = [0 as c_char; 128];
        (unsafe { IORegistryEntryGetName(self.0, buf.as_mut_ptr()) } == 0).then(|| {
            unsafe { CStr::from_ptr(buf.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        })
    }

    /// Returns an owned properties dictionary.
    pub fn properties(&self) -> Option<Owned> {
        let mut props: CFMutableDictionaryRef = std::ptr::null_mut();
        let kr = unsafe {
            IORegistryEntryCreateCFProperties(self.0, &mut props, kCFAllocatorDefault, 0)
        };
        (kr == 0 && !props.is_null()).then(|| Owned(props as CFTypeRef))
    }
}

pub fn services(class: &str) -> Vec<Service> {
    let Ok(c) = CString::new(class) else {
        return Vec::new();
    };
    let mut it: IoObject = 0;
    // IOServiceGetMatchingServices consumes the matching dictionary.
    let kr = unsafe {
        IOServiceGetMatchingServices(kIOMainPortDefault, IOServiceMatching(c.as_ptr()), &mut it)
    };
    if kr != 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    loop {
        let o = unsafe { IOIteratorNext(it) };
        if o == 0 {
            break;
        }
        out.push(Service(o));
    }
    unsafe { IOObjectRelease(it) };
    out
}

#[derive(Debug, Default, Clone)]
pub struct DvfsTables {
    pub ecpu: Vec<u32>,
    pub pcpu: Vec<u32>,
    pub gpu: Vec<u32>,
}

/// Frequency tables from the `pmgr` node (M1–M4 layout: states1 = ECPU, states5 = PCPU, states9 = GPU).
pub fn dvfs_tables() -> DvfsTables {
    let mut t = DvfsTables::default();
    for s in services("AppleARMIODevice") {
        if s.name().as_deref() != Some("pmgr") {
            continue;
        }
        let Some(props) = s.properties() else {
            continue;
        };
        let d = props.0 as CFDictionaryRef;
        let get = |k: &str| {
            cf::dict_get(d, k)
                .and_then(cf::to_data)
                .map(|b| calc::parse_freq_table(&b))
                .unwrap_or_default()
        };
        t.ecpu = get("voltage-states1-sram");
        t.pcpu = get("voltage-states5-sram");
        t.gpu = get("voltage-states9");
    }
    t
}

pub fn gpu_mem_used() -> Option<u64> {
    services("IOAccelerator").into_iter().find_map(|s| {
        let props = s.properties()?;
        let stats = cf::dict_get(props.0 as CFDictionaryRef, "PerformanceStatistics")
            .and_then(cf::to_dict)?;
        cf::dict_get(stats, "In use system memory")
            .and_then(cf::to_i64)
            .map(|v| v as u64)
    })
}

/// Cumulative bytes (read, written) across block storage drivers.
pub fn disk_totals() -> Option<(u64, u64)> {
    let (mut r, mut w, mut any) = (0u64, 0u64, false);
    for s in services("IOBlockStorageDriver") {
        let Some(props) = s.properties() else {
            continue;
        };
        let Some(stats) =
            cf::dict_get(props.0 as CFDictionaryRef, "Statistics").and_then(cf::to_dict)
        else {
            continue;
        };
        r += cf::dict_get(stats, "Bytes (Read)")
            .and_then(cf::to_i64)
            .unwrap_or(0) as u64;
        w += cf::dict_get(stats, "Bytes (Write)")
            .and_then(cf::to_i64)
            .unwrap_or(0) as u64;
        any = true;
    }
    any.then_some((r, w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_registry_reads() {
        let t = dvfs_tables();
        // Apple Silicon always exposes pmgr tables; Intel Macs return empty.
        if std::env::consts::ARCH == "aarch64" {
            assert!(!t.ecpu.is_empty() && !t.pcpu.is_empty(), "{t:?}");
        }
        assert!(disk_totals().is_some());
    }
}

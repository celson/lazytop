use std::ffi::CString;

use super::calc::VmCounts;

pub fn sysctl_raw(name: &str) -> Option<Vec<u8>> {
    let c = CString::new(name).ok()?;
    let mut len: libc::size_t = 0;
    if unsafe {
        libc::sysctlbyname(
            c.as_ptr(),
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    let mut buf = vec![0u8; len];
    if unsafe {
        libc::sysctlbyname(
            c.as_ptr(),
            buf.as_mut_ptr() as *mut _,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    buf.truncate(len);
    Some(buf)
}

pub fn sysctl_string(name: &str) -> Option<String> {
    let b = sysctl_raw(name)?;
    Some(
        String::from_utf8_lossy(&b)
            .trim_end_matches('\0')
            .trim()
            .to_string(),
    )
}

pub fn sysctl_u32(name: &str) -> Option<u32> {
    let b = sysctl_raw(name)?;
    Some(u32::from_ne_bytes(b.get(..4)?.try_into().ok()?))
}

pub fn sysctl_u64(name: &str) -> Option<u64> {
    let b = sysctl_raw(name)?;
    match b.len() {
        8 => Some(u64::from_ne_bytes(b.try_into().ok()?)),
        4 => Some(u32::from_ne_bytes(b.try_into().ok()?) as u64),
        _ => None,
    }
}

/// `struct xsw_usage` → (used, total)
pub fn swap() -> Option<(u64, u64)> {
    let b = sysctl_raw("vm.swapusage")?;
    let total = u64::from_ne_bytes(b.get(0..8)?.try_into().ok()?);
    let used = u64::from_ne_bytes(b.get(16..24)?.try_into().ok()?);
    Some((used, total))
}

#[repr(C)]
#[derive(Default)]
struct VmStatistics64 {
    free_count: u32,
    active_count: u32,
    inactive_count: u32,
    wire_count: u32,
    zero_fill_count: u64,
    reactivations: u64,
    pageins: u64,
    pageouts: u64,
    faults: u64,
    cow_faults: u64,
    lookups: u64,
    hits: u64,
    purges: u64,
    purgeable_count: u32,
    speculative_count: u32,
    decompressions: u64,
    compressions: u64,
    swapins: u64,
    swapouts: u64,
    compressor_page_count: u32,
    throttled_count: u32,
    external_page_count: u32,
    internal_page_count: u32,
    total_uncompressed_pages_in_compressor: u64,
}

unsafe extern "C" {
    fn mach_host_self() -> u32;
    fn host_statistics64(host: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
}

const HOST_VM_INFO64: i32 = 4;

pub fn vm_counts() -> Option<VmCounts> {
    let mut s = VmStatistics64::default();
    let mut count = (std::mem::size_of::<VmStatistics64>() / 4) as u32;
    let kr = unsafe {
        host_statistics64(
            mach_host_self(),
            HOST_VM_INFO64,
            &mut s as *mut _ as *mut i32,
            &mut count,
        )
    };
    (kr == 0).then_some(VmCounts {
        free: s.free_count as u64,
        active: s.active_count as u64,
        inactive: s.inactive_count as u64,
        wired: s.wire_count as u64,
        speculative: s.speculative_count as u64,
        purgeable: s.purgeable_count as u64,
        compressor: s.compressor_page_count as u64,
        internal: s.internal_page_count as u64,
        external: s.external_page_count as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_sysctl_and_vm() {
        assert!(sysctl_string("machdep.cpu.brand_string").is_some());
        assert!(sysctl_u64("hw.memsize").unwrap() > 0);
        assert!(sysctl_u64("hw.pagesize").unwrap() >= 4096);
        let vm = vm_counts().unwrap();
        assert!(vm.wired > 0 && vm.internal > 0);
        assert!(swap().is_some());
        let lvl = sysctl_u32("kern.memorystatus_level").unwrap();
        assert!(lvl <= 100);
    }
}

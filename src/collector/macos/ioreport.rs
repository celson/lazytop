//! IOReport (private libIOReport.dylib) sampler — same technique as `macmon`, no root needed.
use std::time::Instant;

use anyhow::{Context, bail};
use core_foundation::array::{
    CFArrayAppendValue, CFArrayCreateMutable, CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef,
    kCFTypeArrayCallBacks,
};
use core_foundation::base::{CFRelease, CFTypeRef, TCFType, kCFAllocatorDefault};
use core_foundation::dictionary::{
    CFDictionaryCreateMutableCopy, CFDictionaryGetCount, CFDictionaryRef, CFDictionarySetValue,
    CFMutableDictionaryRef,
};
use core_foundation::string::CFStringRef;

use super::calc;
use super::cf::{self, Owned};
use super::iokit::{self, DvfsTables};
use crate::model::{Bandwidth, ClusterKind, CpuCluster, PowerStats};

type Subscription = *const std::ffi::c_void;

#[link(name = "IOReport", kind = "dylib")]
unsafe extern "C" {
    fn IOReportCopyChannelsInGroup(
        group: CFStringRef,
        subgroup: CFStringRef,
        a: u64,
        b: u64,
        c: u64,
    ) -> CFDictionaryRef;
    fn IOReportMergeChannels(a: CFDictionaryRef, b: CFDictionaryRef, nil: CFTypeRef);
    fn IOReportCreateSubscription(
        a: *const std::ffi::c_void,
        desired: CFMutableDictionaryRef,
        subbed: *mut CFMutableDictionaryRef,
        id: u64,
        b: CFTypeRef,
    ) -> Subscription;
    fn IOReportCreateSamples(
        s: Subscription,
        subbed: CFMutableDictionaryRef,
        a: CFTypeRef,
    ) -> CFDictionaryRef;
    fn IOReportCreateSamplesDelta(
        prev: CFDictionaryRef,
        cur: CFDictionaryRef,
        a: CFTypeRef,
    ) -> CFDictionaryRef;
    fn IOReportChannelGetGroup(ch: CFDictionaryRef) -> CFStringRef;
    fn IOReportChannelGetSubGroup(ch: CFDictionaryRef) -> CFStringRef;
    fn IOReportChannelGetChannelName(ch: CFDictionaryRef) -> CFStringRef;
    fn IOReportChannelGetUnitLabel(ch: CFDictionaryRef) -> CFStringRef;
    fn IOReportSimpleGetIntegerValue(ch: CFDictionaryRef, a: i32) -> i64;
    fn IOReportStateGetCount(ch: CFDictionaryRef) -> i32;
    fn IOReportStateGetNameForIndex(ch: CFDictionaryRef, i: i32) -> CFStringRef;
    fn IOReportStateGetResidency(ch: CFDictionaryRef, i: i32) -> i64;
}

pub type Groups = [(&'static str, Option<&'static str>)];

/// Cheap groups, sampled every tick.
pub const CORE_GROUPS: &Groups = &[
    ("Energy Model", None),
    ("CPU Stats", Some("CPU Core Performance States")),
    ("GPU Stats", Some("GPU Performance States")),
];

/// The PMP coprocessor costs ~3.7 ms per sample, so bandwidth gets its own subscription
/// that callers can sample less often.
pub const BANDWIDTH_GROUPS: &Groups = &[("PMP", Some("DCS BW"))];

#[derive(Debug, Default, Clone)]
pub struct IoSample {
    pub clusters: Vec<CpuCluster>,
    pub gpu_usage: Option<f64>,
    pub gpu_mhz: Option<u32>,
    pub power: PowerStats,
    pub bandwidth: Option<Bandwidth>,
}

pub struct IoReport {
    sub: Subscription,
    subbed: CFMutableDictionaryRef,
    _channels: Owned,
    prev: Option<(Owned, Instant)>,
    dvfs: DvfsTables,
}

/// Keeps only the channels `calc::wanted_channel` accepts in the `IOReportChannels` array.
fn prune_channels(dict: CFMutableDictionaryRef) {
    let Some(arr) = cf::dict_get(dict as CFDictionaryRef, "IOReportChannels") else {
        return;
    };
    let arr = arr as CFArrayRef;
    let kept = unsafe { CFArrayCreateMutable(kCFAllocatorDefault, 0, &kCFTypeArrayCallBacks) };
    for i in 0..unsafe { CFArrayGetCount(arr) } {
        let ch = unsafe { CFArrayGetValueAtIndex(arr, i) } as CFDictionaryRef;
        let group = cf::to_string(unsafe { IOReportChannelGetGroup(ch) }).unwrap_or_default();
        let sub = cf::to_string(unsafe { IOReportChannelGetSubGroup(ch) }).unwrap_or_default();
        let name = cf::to_string(unsafe { IOReportChannelGetChannelName(ch) }).unwrap_or_default();
        if calc::wanted_channel(&group, &sub, &name) {
            unsafe { CFArrayAppendValue(kept, ch as *const std::ffi::c_void) };
        }
    }
    let key = cf::cfstr("IOReportChannels");
    unsafe {
        CFDictionarySetValue(
            dict,
            key.as_concrete_TypeRef() as *const _,
            kept as *const _,
        );
        CFRelease(kept as CFTypeRef);
    }
}

impl Drop for IoReport {
    fn drop(&mut self) {
        // Both come from Create-rule calls.
        unsafe {
            if !self.subbed.is_null() {
                CFRelease(self.subbed as CFTypeRef);
            }
            if !self.sub.is_null() {
                CFRelease(self.sub as CFTypeRef);
            }
        }
    }
}

// The subscription is only touched from the collector thread.
unsafe impl Send for IoReport {}

impl IoReport {
    /// Subscribes to every group lazytop uses.
    pub fn new() -> anyhow::Result<Self> {
        Self::with_groups(&[CORE_GROUPS, BANDWIDTH_GROUPS].concat())
    }

    pub fn with_groups(groups: &Groups) -> anyhow::Result<Self> {
        let mut merged: Option<CFDictionaryRef> = None;
        let mut owned = Vec::new();
        for &(g, sg) in groups {
            let gs = cf::cfstr(g);
            let sgs = sg.map(cf::cfstr);
            let ch = unsafe {
                IOReportCopyChannelsInGroup(
                    gs.as_concrete_TypeRef(),
                    sgs.as_ref()
                        .map(|s| s.as_concrete_TypeRef())
                        .unwrap_or(std::ptr::null()),
                    0,
                    0,
                    0,
                )
            };
            if ch.is_null() {
                crate::log::warn_once(&format!("ioreport-{g}"), "channel group not present");
                continue;
            }
            match merged {
                None => merged = Some(ch),
                Some(m) => unsafe { IOReportMergeChannels(m, ch, std::ptr::null()) },
            }
            owned.push(Owned(ch as CFTypeRef));
        }
        let merged = merged.context("no IOReport channel groups available")?;
        let channels = Owned(unsafe {
            CFDictionaryCreateMutableCopy(kCFAllocatorDefault, CFDictionaryGetCount(merged), merged)
        } as CFTypeRef);
        drop(owned);
        prune_channels(channels.0 as CFMutableDictionaryRef);
        let mut subbed: CFMutableDictionaryRef = std::ptr::null_mut();
        let sub = unsafe {
            IOReportCreateSubscription(
                std::ptr::null(),
                channels.0 as CFMutableDictionaryRef,
                &mut subbed,
                0,
                std::ptr::null(),
            )
        };
        if sub.is_null() {
            bail!("IOReportCreateSubscription failed");
        }
        Ok(Self {
            sub,
            subbed,
            _channels: channels,
            prev: None,
            dvfs: iokit::dvfs_tables(),
        })
    }

    fn raw(&self) -> Owned {
        Owned(
            unsafe { IOReportCreateSamples(self.sub, self.subbed, std::ptr::null()) } as CFTypeRef,
        )
    }

    pub fn sample(&mut self) -> anyhow::Result<IoSample> {
        let now = Instant::now();
        let cur = self.raw();
        if cur.0.is_null() {
            bail!("IOReportCreateSamples returned null");
        }
        let Some((prev, pt)) = self.prev.replace((cur, now)) else {
            return Ok(IoSample::default());
        };
        let cur = &self.prev.as_ref().unwrap().0;
        let dt = now.saturating_duration_since(pt).as_secs_f64();
        let delta =
            Owned(
                unsafe { IOReportCreateSamplesDelta(prev.0 as _, cur.0 as _, std::ptr::null()) }
                    as CFTypeRef,
            );
        if delta.0.is_null() {
            bail!("IOReportCreateSamplesDelta returned null");
        }
        Ok(self.parse(delta.0 as CFDictionaryRef, dt))
    }

    fn parse(&self, delta: CFDictionaryRef, dt: f64) -> IoSample {
        let mut out = IoSample::default();
        let mut e_states: Vec<Vec<(String, i64)>> = Vec::new();
        let mut p_states: Vec<Vec<(String, i64)>> = Vec::new();
        let mut agents: Vec<(String, f64)> = Vec::new();
        let Some(arr) = cf::dict_get(delta, "IOReportChannels") else {
            return out;
        };
        let arr = arr as CFArrayRef;
        for i in 0..unsafe { CFArrayGetCount(arr) } {
            let ch = unsafe { CFArrayGetValueAtIndex(arr, i) } as CFDictionaryRef;
            let group = cf::to_string(unsafe { IOReportChannelGetGroup(ch) }).unwrap_or_default();
            let sub = cf::to_string(unsafe { IOReportChannelGetSubGroup(ch) }).unwrap_or_default();
            let name =
                cf::to_string(unsafe { IOReportChannelGetChannelName(ch) }).unwrap_or_default();
            let unit =
                cf::to_string(unsafe { IOReportChannelGetUnitLabel(ch) }).unwrap_or_default();
            let states = || -> Vec<(String, i64)> {
                (0..unsafe { IOReportStateGetCount(ch) })
                    .map(|j| {
                        (
                            cf::to_string(unsafe { IOReportStateGetNameForIndex(ch, j) })
                                .unwrap_or_default(),
                            unsafe { IOReportStateGetResidency(ch, j) },
                        )
                    })
                    .collect()
            };
            match group.as_str() {
                "Energy Model" => {
                    let w = calc::watts_from_energy(
                        unsafe { IOReportSimpleGetIntegerValue(ch, 0) },
                        &unit,
                        dt,
                    );
                    let slot = if name == "GPU Energy" {
                        &mut out.power.gpu_w
                    } else if name.ends_with("CPU Energy") {
                        &mut out.power.cpu_w
                    } else if name.starts_with("ANE") {
                        &mut out.power.ane_w
                    } else if name.starts_with("DRAM") {
                        &mut out.power.dram_w
                    } else {
                        continue;
                    };
                    if let Some(w) = w {
                        *slot = Some(slot.unwrap_or(0.0) + w);
                    }
                }
                "CPU Stats" if sub == "CPU Core Performance States" => {
                    if name.starts_with("ECPU") {
                        e_states.push(states());
                    } else if name.starts_with("PCPU") {
                        p_states.push(states());
                    }
                }
                "GPU Stats" if sub == "GPU Performance States" && name == "GPUPH" => {
                    let (u, mhz) = calc::residency_usage_mhz(&states(), &self.dvfs.gpu);
                    out.gpu_usage = Some(u);
                    out.gpu_mhz = mhz;
                }
                // DCS BW channels are residency histograms over bandwidth bins ("  12GB/s").
                "PMP" if sub == "DCS BW" && name.ends_with("RD+WR") => {
                    if let Some(gbps) = calc::histogram_mean_gbps(&states()) {
                        agents.push((name, gbps * 1e9));
                    }
                }
                _ => {}
            }
        }
        for (kind, cores, table) in [
            (ClusterKind::Efficiency, &e_states, &self.dvfs.ecpu),
            (ClusterKind::Performance, &p_states, &self.dvfs.pcpu),
        ] {
            if cores.is_empty() {
                continue;
            }
            let per: Vec<(f64, Option<u32>)> = cores
                .iter()
                .map(|s| calc::residency_usage_mhz(s, table))
                .collect();
            let usage = per.iter().map(|p| p.0).sum::<f64>() / per.len() as f64;
            let mhz = per.iter().filter_map(|p| p.1).max();
            out.clusters.push(CpuCluster { kind, usage, mhz });
        }
        let cpu_busy = out.clusters.iter().any(|c| c.usage > 0.01);
        if cpu_busy && out.power.cpu_w == Some(0.0) {
            crate::log::warn_once(
                "ioreport-cpu-energy",
                "CPU energy counter not updating; CPU power shown as —",
            );
        }
        out.power = calc::finalize_power(std::mem::take(&mut out.power), cpu_busy);
        out.bandwidth = (!agents.is_empty()).then(|| calc::bandwidth_from_agents(&agents));
        out
    }

    /// Dumps every channel (group/subgroup/name/unit) to the log; used to tune bandwidth mapping.
    pub fn log_channels(&self) {
        let raw = self.raw();
        let Some(arr) = cf::dict_get(raw.0 as CFDictionaryRef, "IOReportChannels") else {
            return;
        };
        let arr = arr as CFArrayRef;
        for i in 0..unsafe { CFArrayGetCount(arr) } {
            let ch = unsafe { CFArrayGetValueAtIndex(arr, i) } as CFDictionaryRef;
            crate::log::info(&format!(
                "ioreport channel: {} / {} / {} [{}]",
                cf::to_string(unsafe { IOReportChannelGetGroup(ch) }).unwrap_or_default(),
                cf::to_string(unsafe { IOReportChannelGetSubGroup(ch) }).unwrap_or_default(),
                cf::to_string(unsafe { IOReportChannelGetChannelName(ch) }).unwrap_or_default(),
                cf::to_string(unsafe { IOReportChannelGetUnitLabel(ch) }).unwrap_or_default(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_two_samples() {
        if !crate::collector::macos::hw_smoke() {
            return;
        }
        let mut r = IoReport::new().expect("subscribe");
        let first = r.sample().unwrap();
        assert!(first.clusters.is_empty(), "first sample has no delta");
        std::thread::sleep(std::time::Duration::from_millis(300));
        let s = r.sample().unwrap();
        assert!(
            s.clusters.iter().any(|c| c.kind == ClusterKind::Efficiency),
            "{s:?}"
        );
        assert!(s.clusters.iter().all(|c| (0.0..=1.0).contains(&c.usage)));
        assert!(s.power.gpu_w.is_some(), "{s:?}");
        // frozen CPU energy counters must not be reported as 0 W
        assert_ne!(s.power.cpu_w, Some(0.0), "{s:?}");
        assert!(s.gpu_usage.is_some());
        drop(r); // releasing the subscription must not crash
        let bw = s.bandwidth.expect("DCS BW histogram");
        assert!(bw.total > 0.0 && bw.total < 200e9, "{bw:?}");
        assert!(bw.cpu > 0.0, "{bw:?}");
    }
}

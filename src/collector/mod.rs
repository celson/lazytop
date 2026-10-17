pub mod ai;
pub mod common;
pub mod demo;
pub mod macos;
pub mod rate;

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::model::*;

pub trait Collector: Send {
    fn sample(&mut self) -> Snapshot;
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemDetail {
    pub used: u64,
    pub free: u64,
    pub wired: u64,
    pub active: u64,
    pub compressed: u64,
    pub app: u64,
    pub cached: u64,
    pub pressure: Option<f64>,
    pub swap_used: u64,
    pub swap_total: u64,
}

/// What a platform backend adds on top of the common sysinfo data.
#[derive(Debug, Clone, Default)]
pub struct PlatformData {
    pub chip: Option<String>,
    pub cores_e: Option<usize>,
    pub cores_p: Option<usize>,
    /// Empty → derived from per-core usage as one `All` cluster.
    pub clusters: Vec<CpuCluster>,
    pub gpu: Option<GpuStats>,
    pub power: Option<PowerStats>,
    pub mem: Option<MemDetail>,
    pub bandwidth: Option<Bandwidth>,
    pub sensors: Vec<SensorGroup>,
    pub fans: Vec<Fan>,
    /// Cumulative (read, written) bytes since boot.
    pub disk_totals: Option<(u64, u64)>,
}

pub trait Platform: Send {
    /// Receives sysinfo per-core usage before `sample` (default: ignored).
    fn set_per_core(&mut self, _per_core: &[f64]) {}
    fn sample(&mut self) -> PlatformData;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Control {
    SetInterval(Duration),
    Pause(bool),
    Quit,
}

/// Runs `c` on its own thread. The thread exits when the receiver is dropped or on `Control::Quit`.
pub fn spawn(
    mut c: Box<dyn Collector>,
    interval: Duration,
) -> (Receiver<Snapshot>, Sender<Control>) {
    let (tx, rx) = mpsc::channel();
    let (ctl_tx, ctl_rx) = mpsc::channel();
    thread::Builder::new()
        .name("lazytop-collector".into())
        .spawn(move || {
            let mut interval = interval;
            let mut paused = false;
            loop {
                let started = Instant::now();
                if !paused && tx.send(c.sample()).is_err() {
                    return;
                }
                // Sleep for the rest of the interval, waking up on control messages.
                loop {
                    let wait = interval.saturating_sub(started.elapsed());
                    match ctl_rx.recv_timeout(wait) {
                        Ok(Control::Quit) | Err(RecvTimeoutError::Disconnected) => return,
                        Ok(Control::Pause(p)) => paused = p,
                        Ok(Control::SetInterval(d)) => interval = d,
                        Err(RecvTimeoutError::Timeout) => break,
                    }
                    if !paused && started.elapsed() >= interval {
                        break;
                    }
                }
            }
        })
        .expect("spawn collector thread");
    (rx, ctl_tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(u32);
    impl Collector for Fake {
        fn sample(&mut self) -> Snapshot {
            self.0 += 1;
            let mut s = Snapshot::default();
            s.host.cores_total = self.0 as usize;
            s
        }
    }

    #[test]
    fn sends_snapshots_and_pauses() {
        let (rx, ctl) = spawn(Box::new(Fake(0)), Duration::from_millis(20));
        let a = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let b = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(b.host.cores_total > a.host.cores_total);
        ctl.send(Control::Pause(true)).unwrap();
        while rx.recv_timeout(Duration::from_millis(60)).is_ok() {}
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        ctl.send(Control::Pause(false)).unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(1)).is_ok());
        ctl.send(Control::Quit).unwrap();
    }
}

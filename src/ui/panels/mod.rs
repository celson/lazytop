pub mod ai;
pub mod cpu;
pub mod gpu;
pub mod header;
pub mod help;
pub mod memory;
pub mod netdisk;
pub mod processes;
pub mod sensors;

use std::collections::HashSet;

use crate::history::History;
use crate::model::{SensorKind, Snapshot};
use crate::ui::theme::Theme;

pub struct Ctx<'a> {
    pub snap: &'a Snapshot,
    pub hist: &'a History,
    pub theme: &'a Theme,
    pub focused: bool,
    pub zoomed: bool,
    pub idx: usize,
}

#[derive(Debug, Default, Clone)]
pub struct SensorView {
    pub selected: usize,
    pub expanded: HashSet<SensorKind>,
}

pub fn gpu_has_data(s: &Snapshot) -> bool {
    s.gpu.is_some() || s.power.as_ref().is_some_and(|p| p.ane_w.is_some()) || s.bandwidth.is_some()
}

pub fn sensors_have_data(s: &Snapshot) -> bool {
    !s.sensors.is_empty() || !s.fans.is_empty()
}

#[cfg(test)]
pub(crate) mod fixture {
    use crate::collector::{Collector, demo::DemoCollector};
    use crate::history::History;
    use crate::model::Snapshot;

    pub fn demo() -> (Snapshot, History) {
        let mut c = DemoCollector::new(42);
        let mut h = History::new(120);
        let mut s = c.sample();
        for _ in 0..30 {
            s = c.sample();
            h.push(&s);
        }
        (s, h)
    }
}

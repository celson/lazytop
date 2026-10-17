use std::collections::{HashMap, VecDeque};

use crate::model::{ClusterKind, SensorKind, Snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Series {
    CpuE,
    CpuP,
    CpuAll,
    Gpu,
    Bandwidth,
    MemUsed,
    NetRx,
    NetTx,
    DiskRead,
    DiskWrite,
    TempCpu,
    TempGpu,
    TempBattery,
}

#[derive(Debug)]
pub struct History {
    cap: usize,
    data: HashMap<Series, VecDeque<f64>>,
}

impl History {
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            data: HashMap::new(),
        }
    }

    fn put(&mut self, s: Series, v: f64) {
        if !v.is_finite() {
            return;
        }
        let q = self.data.entry(s).or_default();
        if q.len() == self.cap {
            q.pop_front();
        }
        q.push_back(v);
    }

    pub fn push(&mut self, s: &Snapshot) {
        for c in &s.cpu.clusters {
            let series = match c.kind {
                ClusterKind::Efficiency => Series::CpuE,
                ClusterKind::Performance => Series::CpuP,
                ClusterKind::All => Series::CpuAll,
            };
            self.put(series, c.usage);
        }
        if s.cpu.clusters.is_empty() {
            self.put(Series::CpuAll, s.cpu.total);
        }
        if let Some(g) = &s.gpu {
            self.put(Series::Gpu, g.usage);
        }
        if let Some(b) = &s.bandwidth {
            self.put(Series::Bandwidth, b.total);
        }
        if s.mem.total > 0 {
            self.put(Series::MemUsed, s.mem.used as f64 / s.mem.total as f64);
        }
        self.put(Series::NetRx, s.net.rx);
        self.put(Series::NetTx, s.net.tx);
        if let Some(r) = s.disk.read {
            self.put(Series::DiskRead, r);
        }
        if let Some(w) = s.disk.write {
            self.put(Series::DiskWrite, w);
        }
        for g in &s.sensors {
            let series = match g.kind {
                SensorKind::Cpu => Series::TempCpu,
                SensorKind::Gpu => Series::TempGpu,
                SensorKind::Battery => Series::TempBattery,
                _ => continue,
            };
            if let Some(avg) = g.avg() {
                self.put(series, avg);
            }
        }
    }

    pub fn points(&self, s: Series) -> Vec<(f64, f64)> {
        self.data
            .get(&s)
            .map(|q| q.iter().enumerate().map(|(i, v)| (i as f64, *v)).collect())
            .unwrap_or_default()
    }

    pub fn last(&self, s: Series) -> Option<f64> {
        self.data.get(&s).and_then(|q| q.back().copied())
    }

    pub fn max(&self, s: Series) -> f64 {
        self.data
            .get(&s)
            .and_then(|q| q.iter().copied().reduce(f64::max))
            .unwrap_or(0.0)
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn snap(e: f64, rx: f64) -> Snapshot {
        Snapshot {
            cpu: CpuStats {
                clusters: vec![CpuCluster {
                    kind: ClusterKind::Efficiency,
                    usage: e,
                    mhz: None,
                }],
                total: e,
                ..Default::default()
            },
            net: NetStats {
                rx,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn push_and_points() {
        let mut h = History::new(3);
        h.push(&snap(0.1, 10.0));
        h.push(&snap(0.2, 20.0));
        assert_eq!(h.points(Series::CpuE), vec![(0.0, 0.1), (1.0, 0.2)]);
        assert_eq!(h.last(Series::NetRx), Some(20.0));
        assert_eq!(h.max(Series::NetRx), 20.0);
    }

    #[test]
    fn ring_drops_oldest() {
        let mut h = History::new(2);
        for i in 0..5 {
            h.push(&snap(i as f64 / 10.0, 0.0));
        }
        assert_eq!(h.points(Series::CpuE), vec![(0.0, 0.3), (1.0, 0.4)]);
    }

    #[test]
    fn missing_series_is_empty() {
        let h = History::new(2);
        assert!(h.points(Series::Gpu).is_empty());
        assert_eq!(h.last(Series::Gpu), None);
        assert_eq!(h.max(Series::Gpu), 0.0);
    }
}

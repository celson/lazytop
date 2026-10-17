use std::time::Instant;

/// Turns a monotonically increasing counter into a per-second rate.
#[derive(Debug, Default)]
pub struct Counter {
    prev: Option<(u64, Instant)>,
}

impl Counter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns units/second since the previous call, `None` on the first call or dt == 0.
    /// A counter that went backwards (reset) yields `Some(0.0)` and rebases.
    pub fn update(&mut self, value: u64, now: Instant) -> Option<f64> {
        let out = match self.prev {
            None => None,
            Some((pv, pt)) => {
                let dt = now.saturating_duration_since(pt).as_secs_f64();
                if dt <= 0.0 {
                    return None; // keep previous base
                }
                Some(value.saturating_sub(pv) as f64 / dt)
            }
        };
        self.prev = Some((value, now));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn first_sample_is_none() {
        let mut c = Counter::new();
        assert_eq!(c.update(100, Instant::now()), None);
    }

    #[test]
    fn rate_per_second() {
        let mut c = Counter::new();
        let t0 = Instant::now();
        c.update(1_000, t0);
        let r = c.update(3_000, t0 + Duration::from_millis(500)).unwrap();
        assert!((r - 4_000.0).abs() < 1e-6);
    }

    #[test]
    fn counter_reset_yields_zero() {
        let mut c = Counter::new();
        let t0 = Instant::now();
        c.update(5_000, t0);
        assert_eq!(c.update(10, t0 + Duration::from_secs(1)), Some(0.0));
        // and continues from the new base
        assert_eq!(c.update(110, t0 + Duration::from_secs(2)), Some(100.0));
    }

    #[test]
    fn zero_dt_is_none() {
        let mut c = Counter::new();
        let t0 = Instant::now();
        c.update(1, t0);
        assert_eq!(c.update(2, t0), None);
    }
}

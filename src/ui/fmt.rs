use std::time::Duration;

pub fn dash() -> &'static str {
    "—"
}

const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

fn scaled(v: f64) -> (f64, &'static str) {
    let mut v = v;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    (v, UNITS[i])
}

pub fn bytes(b: u64) -> String {
    let (v, u) = scaled(b as f64);
    match u {
        "B" | "KB" | "MB" => format!("{v:.0} {u}"),
        "GB" => format!("{v:.1} {u}"),
        _ => format!("{v:.2} {u}"),
    }
}

pub fn rate(bps: f64) -> String {
    if !bps.is_finite() {
        return dash().into();
    }
    // network/disk/bandwidth rates use decimal units like the screenshot
    let mut v = bps.max(0.0);
    let mut i = 0;
    while v >= 1000.0 && i < UNITS.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    format!("{v:.0} {}/s", UNITS[i])
}

pub fn pct(r: f64) -> String {
    format!("{:.0}%", (r * 100.0).max(0.0))
}

pub fn mhz(m: Option<u32>) -> String {
    m.map(|m| format!("{m} MHz"))
        .unwrap_or_else(|| dash().into())
}

pub fn watts(w: Option<f64>) -> String {
    w.map(|w| format!("{w:.1} W"))
        .unwrap_or_else(|| dash().into())
}

pub fn temp(c: f64) -> String {
    format!("{c:.0}°C")
}

pub fn uptime(d: Duration) -> String {
    let s = d.as_secs();
    let (days, h, m) = (s / 86_400, (s % 86_400) / 3600, (s % 3600) / 60);
    if days > 0 {
        format!("{days}d {h}h")
    } else {
        format!("{h}h {m}m")
    }
}

/// 35431 → "35.431" (thousands separator like the screenshot's PID column)
pub fn thousands(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Truncates to `width` display columns (CJK = 2), adding `…` when cut.
pub fn truncate(s: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let total: usize = s.chars().map(|c| c.width().unwrap_or(0)).sum();
    if total <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if w + cw > width - 1 {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_units() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(44 * 1024 * 1024), "44 MB");
        assert_eq!(bytes((10.8 * (1u64 << 30) as f64) as u64), "10.8 GB");
        assert_eq!(bytes(36_650 * (1u64 << 30)), "35.79 TB");
    }

    #[test]
    fn rates() {
        assert_eq!(rate(5_000.0), "5 KB/s");
        assert_eq!(rate(13e9), "13 GB/s");
        assert_eq!(rate(0.0), "0 B/s");
        assert_eq!(rate(f64::NAN), "—");
    }

    #[test]
    fn misc() {
        assert_eq!(pct(0.126), "13%");
        assert_eq!(mhz(Some(1688)), "1688 MHz");
        assert_eq!(mhz(None), "—");
        assert_eq!(watts(Some(4.66)), "4.7 W");
        assert_eq!(watts(None), "—");
        assert_eq!(temp(45.4), "45°C");
        assert_eq!(
            uptime(Duration::from_secs(3 * 86400 + 2 * 3600 + 5 * 60)),
            "3d 2h"
        );
        assert_eq!(uptime(Duration::from_secs(2 * 3600 + 5 * 60)), "2h 5m");
        assert_eq!(thousands(35431), "35.431");
    }

    #[test]
    fn truncate_handles_multibyte() {
        assert_eq!(truncate("screencaptureui", 6), "scree…");
        assert_eq!(truncate("日本語のプロセス", 5), "日本…");
        assert_eq!(truncate("abc", 3), "abc");
        assert_eq!(truncate("abc", 0), "");
        assert_eq!(truncate("éé", 1), "…");
    }
}

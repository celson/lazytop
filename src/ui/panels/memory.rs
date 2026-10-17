use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::Ctx;
use crate::history::Series;
use crate::ui::{
    fmt,
    theme::Theme,
    widgets::{LabeledBar, panel_block, render_history},
};

fn kv(label: &str, value: String, swatch: Option<Color>, width: u16, t: &Theme) -> Line<'static> {
    let mut spans = Vec::new();
    if let Some(c) = swatch {
        spans.push(Span::styled("■ ", Style::default().fg(c)));
    }
    let lw = label.chars().count() + if swatch.is_some() { 2 } else { 0 };
    spans.push(Span::styled(label.to_string(), Style::default().fg(t.dim)));
    spans.push(Span::raw(" ".repeat(
        (width as usize).saturating_sub(lw + value.chars().count()),
    )));
    spans.push(Span::styled(value, Style::default().fg(t.fg)));
    Line::from(spans)
}

pub fn render(f: &mut Frame, area: Rect, c: &Ctx) {
    let t = c.theme;
    let s = c.snap;
    let m = &s.mem;
    let block = panel_block("Memory & bandwidth", Some(c.idx), c.focused, t);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
            .spacing(2)
            .areas(inner);

    // left: total + segmented bar + breakdown
    let w = left.width;
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("{} / {}", fmt::bytes(m.used), fmt::bytes(m.total)),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", fmt::pct(m.used as f64 / m.total.max(1) as f64)),
            Style::default().fg(t.dim),
        ),
    ])];
    // segmented bar: wired | active(app) | compressed | free
    let segs: Vec<(u64, Color)> = match (m.wired, m.app, m.compressed) {
        (Some(wd), Some(app), Some(cp)) => vec![
            (wd, t.mem_wired),
            (app, t.mem_active),
            (cp, t.mem_compressed),
        ],
        _ => vec![(m.used, t.mem_active)],
    };
    let mut bar = Vec::new();
    let mut used_cols = 0usize;
    for (v, col) in segs {
        let n = ((v as f64 / m.total.max(1) as f64) * w as f64).round() as usize;
        let n = n.min((w as usize).saturating_sub(used_cols));
        bar.push(Span::styled("━".repeat(n), Style::default().fg(col)));
        used_cols += n;
    }
    bar.push(Span::styled(
        "━".repeat((w as usize).saturating_sub(used_cols)),
        Style::default().fg(t.mem_free),
    ));
    lines.push(Line::from(bar));
    if let (Some(wd), Some(ac), Some(cp)) = (m.wired, m.active, m.compressed) {
        lines.push(kv("Wired", fmt::bytes(wd), Some(t.mem_wired), w, t));
        lines.push(kv("Active", fmt::bytes(ac), Some(t.mem_active), w, t));
        lines.push(kv(
            "Compressed",
            fmt::bytes(cp),
            Some(t.mem_compressed),
            w,
            t,
        ));
    }
    lines.push(kv("Free", fmt::bytes(m.free), Some(t.mem_free), w, t));
    let used_lines = lines.len() as u16;
    let [top, pressure_area, bottom] = Layout::vertical([
        Constraint::Length(used_lines),
        Constraint::Length(2),
        Constraint::Fill(1),
    ])
    .areas(left);
    f.render_widget(Paragraph::new(lines), top);
    if let Some(p) = m.pressure {
        let col = if p > 0.75 {
            t.crit
        } else if p > 0.5 {
            t.warn
        } else {
            t.dim
        };
        f.render_widget(
            LabeledBar {
                label: "Pressure",
                value: fmt::pct(p),
                ratio: p,
                color: col,
                theme: t,
            },
            pressure_area,
        );
    }
    let mut extra = Vec::new();
    if let Some(a) = m.app {
        extra.push(kv("App Memory", fmt::bytes(a), None, w, t));
    }
    if let Some(cch) = m.cached {
        extra.push(kv("Cached", fmt::bytes(cch), None, w, t));
    }
    extra.push(kv(
        "Swap",
        format!("{} / {}", fmt::bytes(m.swap_used), fmt::bytes(m.swap_total)),
        None,
        w,
        t,
    ));
    f.render_widget(Paragraph::new(extra), bottom);

    // right: bandwidth
    let rw = right.width;
    match &s.bandwidth {
        Some(b) => {
            let gb = |v: f64| format!("{:.0} GB/s", v / 1e9);
            let lines = vec![
                Line::styled(
                    "BANDWIDTH",
                    Style::default().fg(t.dim).add_modifier(Modifier::BOLD),
                ),
                kv("Total", gb(b.total), None, rw, t),
                kv("CPU", gb(b.cpu), None, rw, t),
                kv("GPU", gb(b.gpu), None, rw, t),
                kv("Media", gb(b.media), None, rw, t),
                kv("ANE", gb(b.ane), None, rw, t),
                kv("Other", gb(b.other), None, rw, t),
            ];
            let [txt, chart] =
                Layout::vertical([Constraint::Length(7), Constraint::Fill(1)]).areas(right);
            f.render_widget(Paragraph::new(lines), txt);
            let max = c.hist.max(Series::Bandwidth);
            render_history(
                f,
                chart,
                &[(c.hist.points(Series::Bandwidth), t.rx)],
                c.hist.capacity() as f64,
                max * 1.1,
            );
        }
        None => {
            f.render_widget(
                Paragraph::new(vec![
                    Line::styled(
                        "BANDWIDTH",
                        Style::default().fg(t.dim).add_modifier(Modifier::BOLD),
                    ),
                    Line::styled("Bandwidth —", Style::default().fg(t.dim)),
                ]),
                right,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{panels::fixture, test_util::render as draw};

    #[test]
    fn breakdown_and_bandwidth() {
        let (s, h) = fixture::demo();
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: false,
            zoomed: false,
            idx: 5,
        };
        let out = draw(80, 18, |f| render(f, f.area(), &ctx));
        for k in [
            "/ 18.0 GB",
            "Wired",
            "Active",
            "Compressed",
            "Free",
            "Pressure",
            "Swap",
            "BANDWIDTH",
            "GB/s",
        ] {
            assert!(out.contains(k), "missing {k}\n{out}");
        }
    }

    #[test]
    fn linux_like_without_breakdown() {
        let (mut s, h) = fixture::demo();
        s.mem.wired = None;
        s.mem.pressure = None;
        s.bandwidth = None;
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: false,
            zoomed: false,
            idx: 5,
        };
        let out = draw(80, 18, |f| render(f, f.area(), &ctx));
        assert!(!out.contains("Wired"));
        assert!(out.contains("Bandwidth —") || out.contains("BANDWIDTH"));
    }
}

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use super::Ctx;
use crate::history::Series;
use crate::ui::{
    fmt,
    widgets::{LabeledBar, panel_block, render_history},
};

pub fn render(f: &mut Frame, area: Rect, c: &Ctx) {
    let t = c.theme;
    let s = c.snap;
    let block = panel_block("GPU / Media / Neural Engine", Some(c.idx), c.focused, t);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [a, b, cc, d, chart] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Fill(1),
    ])
    .areas(inner);

    let gpu_w = s.power.as_ref().and_then(|p| p.gpu_w);
    let (usage, value) = match &s.gpu {
        Some(g) => (
            g.usage,
            format!(
                "{}  {}  {}",
                fmt::pct(g.usage),
                fmt::watts(gpu_w),
                fmt::mhz(g.mhz)
            ),
        ),
        None => (0.0, fmt::dash().into()),
    };
    f.render_widget(
        LabeledBar {
            label: "GPU",
            value,
            ratio: usage,
            color: t.gpu,
            theme: t,
        },
        a,
    );

    let (mem_ratio, mem_val) = match s.gpu.as_ref().and_then(|g| g.mem_used) {
        Some(u) => {
            let total = s
                .gpu
                .as_ref()
                .and_then(|g| g.mem_total)
                .unwrap_or(s.mem.total)
                .max(1);
            (u as f64 / total as f64, format!("{} in use", fmt::bytes(u)))
        }
        None => (0.0, fmt::dash().into()),
    };
    f.render_widget(
        LabeledBar {
            label: "GPU memory",
            value: mem_val,
            ratio: mem_ratio,
            color: t.gpu,
            theme: t,
        },
        b,
    );

    let ane = s.power.as_ref().and_then(|p| p.ane_w);
    let ane_val = match ane {
        Some(w) if w > 0.0 => fmt::watts(Some(w)),
        _ => fmt::dash().into(),
    };
    // ANE max ≈ 8 W on M-series; used only to scale the bar.
    f.render_widget(
        LabeledBar {
            label: "ANE est.",
            value: ane_val,
            ratio: ane.unwrap_or(0.0) / 8.0,
            color: t.ane,
            theme: t,
        },
        cc,
    );

    let (media_ratio, media_val) = match &s.bandwidth {
        Some(bw) => (
            bw.media / bw.total.max(1.0),
            format!("{:.1} GB/s", bw.media / 1e9),
        ),
        None => (0.0, fmt::dash().into()),
    };
    f.render_widget(
        LabeledBar {
            label: "Media",
            value: media_val,
            ratio: media_ratio,
            color: t.media,
            theme: t,
        },
        d,
    );

    let pts = c.hist.points(Series::Gpu);
    render_history(f, chart, &[(pts, t.gpu)], c.hist.capacity() as f64, 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::Theme;
    use crate::ui::{panels::fixture, test_util::render as draw};

    #[test]
    fn gpu_rows() {
        let (s, h) = fixture::demo();
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: false,
            zoomed: false,
            idx: 4,
        };
        let out = draw(70, 14, |f| render(f, f.area(), &ctx));
        for k in ["GPU", "GPU memory", "ANE", "Media", "MHz", "in use"] {
            assert!(out.contains(k), "missing {k}\n{out}");
        }
    }

    #[test]
    fn missing_values_render_dash() {
        let (mut s, h) = fixture::demo();
        s.power = None;
        s.bandwidth = None;
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: false,
            zoomed: false,
            idx: 4,
        };
        let out = draw(70, 14, |f| render(f, f.area(), &ctx));
        assert!(out.contains('—'));
    }
}

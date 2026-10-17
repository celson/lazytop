use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use super::Ctx;
use crate::history::Series;
use crate::model::ClusterKind;
use crate::ui::{
    fmt,
    widgets::{LabeledBar, panel_block, render_history},
};

pub fn render(f: &mut Frame, area: Rect, c: &Ctx) {
    let t = c.theme;
    let s = c.snap;
    let block = panel_block("CPU", Some(c.idx), c.focused, t);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let bars = s.cpu.clusters.len() as u16 * 2;
    let core_rows = if c.zoomed {
        s.cpu.per_core.len() as u16
    } else {
        0
    };
    let [bars_area, cores_area, chart_area] = Layout::vertical([
        Constraint::Length(bars),
        Constraint::Length(core_rows),
        Constraint::Fill(1),
    ])
    .areas(inner);

    for (i, cl) in s.cpu.clusters.iter().enumerate() {
        let color = match cl.kind {
            ClusterKind::Efficiency => t.e_core,
            ClusterKind::Performance => t.p_core,
            ClusterKind::All => t.accent,
        };
        let value = match cl.mhz {
            Some(m) => format!("{}  {m} MHz", fmt::pct(cl.usage)),
            None => fmt::pct(cl.usage),
        };
        let r = Rect {
            y: bars_area.y + i as u16 * 2,
            height: 2,
            ..bars_area
        };
        if r.bottom() <= bars_area.bottom() {
            f.render_widget(
                LabeledBar {
                    label: cl.kind.label(),
                    value,
                    ratio: cl.usage,
                    color,
                    theme: t,
                },
                r,
            );
        }
    }

    if c.zoomed {
        for (i, u) in s.cpu.per_core.iter().enumerate() {
            let r = Rect {
                y: cores_area.y + i as u16,
                height: 1,
                ..cores_area
            };
            if r.bottom() <= cores_area.bottom() {
                f.render_widget(
                    LabeledBar {
                        label: &format!("core {i}"),
                        value: fmt::pct(*u),
                        ratio: *u,
                        color: t.accent,
                        theme: t,
                    },
                    r,
                );
            }
        }
    }

    let series: Vec<_> = [
        (Series::CpuE, t.e_core),
        (Series::CpuP, t.p_core),
        (Series::CpuAll, t.accent),
    ]
    .into_iter()
    .map(|(sr, col)| (c.hist.points(sr), col))
    .filter(|(p, _)| !p.is_empty())
    .collect();
    render_history(f, chart_area, &series, c.hist.capacity() as f64, 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::Theme;
    use crate::ui::{panels::fixture, test_util::render as draw};

    #[test]
    fn clusters_and_mhz() {
        let (s, h) = fixture::demo();
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: true,
            zoomed: false,
            idx: 3,
        };
        let out = draw(70, 14, |f| render(f, f.area(), &ctx));
        assert!(out.contains("[3] CPU"));
        assert!(out.contains("Efficiency") && out.contains("Performance"));
        assert!(out.contains("MHz"));
    }

    #[test]
    fn zoom_shows_per_core() {
        let (s, h) = fixture::demo();
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: true,
            zoomed: true,
            idx: 3,
        };
        let out = draw(120, 40, |f| render(f, f.area(), &ctx));
        assert!(out.contains("core 0") && out.contains("core 10"), "{out}");
    }
}

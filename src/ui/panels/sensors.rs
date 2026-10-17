use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{Ctx, SensorView};
use crate::history::Series;
use crate::model::SensorKind;
use crate::ui::{
    fmt,
    widgets::{panel_block, render_history},
};

pub fn render(f: &mut Frame, area: Rect, c: &Ctx, v: &SensorView) {
    let t = c.theme;
    let s = c.snap;
    let block = panel_block("Sensors", Some(c.idx), c.focused, t);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width as usize;

    let fans = if s.fans.is_empty() {
        format!("Fans {}", fmt::dash())
    } else {
        let rpms: Vec<String> = s.fans.iter().map(|f| format!("{:.0}", f.rpm)).collect();
        format!("Fans {} rpm", rpms.join(" / "))
    };
    let mut lines = vec![
        Line::styled(fans, Style::default().fg(t.dim)),
        Line::raw(""),
    ];
    for (i, g) in s.sensors.iter().enumerate() {
        let open = v.expanded.contains(&g.kind) || c.zoomed;
        let selected = c.focused && i == v.selected;
        let color = match g.kind {
            SensorKind::Cpu => t.p_core,
            SensorKind::Gpu => t.gpu,
            SensorKind::Battery => t.e_core,
            _ => t.dim,
        };
        let right = format!(
            "avg {} · max {}",
            g.avg().map(fmt::temp).unwrap_or_else(|| fmt::dash().into()),
            g.max().map(fmt::temp).unwrap_or_else(|| fmt::dash().into())
        );
        let left = format!(
            "{} ■ {} ({})",
            if open { "▾" } else { "▸" },
            g.kind.label(),
            g.readings.len()
        );
        let pad = fmt::pad(&left, &right, w);
        let base = if selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", if open { "▾" } else { "▸" }), base),
            Span::styled("■ ", base.fg(color)),
            Span::styled(
                format!("{} ", g.kind.label()),
                base.add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("({})", g.readings.len()), base.fg(t.dim)),
            Span::styled(" ".repeat(pad), base),
            Span::styled(right, base.fg(t.dim)),
        ]));
        if open {
            for (label, val) in &g.readings {
                let l = format!("    {}", fmt::truncate(label, w.saturating_sub(12)));
                let r = fmt::temp(*val);
                let pad = fmt::pad(&l, &r, w);
                lines.push(Line::from(vec![
                    Span::styled(l, Style::default().fg(t.dim)),
                    Span::raw(" ".repeat(pad)),
                    Span::raw(r),
                ]));
            }
        }
    }
    let n = lines.len() as u16;
    let [txt, chart] =
        Layout::vertical([Constraint::Length(n.min(inner.height)), Constraint::Fill(1)])
            .areas(inner);
    f.render_widget(Paragraph::new(lines), txt);
    let series = vec![
        (c.hist.points(Series::TempCpu), t.p_core),
        (c.hist.points(Series::TempGpu), t.gpu),
        (c.hist.points(Series::TempBattery), t.e_core),
    ];
    render_history(f, chart, &series, c.hist.capacity() as f64, 110.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::Theme;
    use crate::ui::{panels::fixture, test_util::render as draw};

    #[test]
    fn groups_collapsed_and_expanded() {
        let (s, h) = fixture::demo();
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: true,
            zoomed: false,
            idx: 7,
        };
        let mut v = SensorView::default();
        let out = draw(70, 12, |f| render(f, f.area(), &ctx, &v));
        assert!(
            out.contains("CPU")
                && out.contains("(3)")
                && out.contains("avg")
                && out.contains("max")
        );
        assert!(out.contains("Fans 0"));
        assert!(!out.contains("Tp01"));
        v.expanded.insert(SensorKind::Cpu);
        let out = draw(70, 14, |f| render(f, f.area(), &ctx, &v));
        assert!(out.contains("Tp01"));
    }

    #[test]
    fn fanless_machine_does_not_invent_fans() {
        let (mut s, h) = fixture::demo();
        s.fans.clear();
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: false,
            zoomed: false,
            idx: 7,
        };
        let out = draw(70, 12, |f| {
            render(f, f.area(), &ctx, &SensorView::default())
        });
        assert!(out.contains("Fans —"), "{out}");
        assert!(!out.contains("0 / 0 rpm"));
    }

    #[test]
    fn stopped_fans_still_listed() {
        let (mut s, h) = fixture::demo();
        s.fans = vec![
            crate::model::Fan {
                label: "Fan 0".into(),
                rpm: 0.0,
            },
            crate::model::Fan {
                label: "Fan 1".into(),
                rpm: 0.0,
            },
        ];
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: false,
            zoomed: false,
            idx: 7,
        };
        let out = draw(70, 12, |f| {
            render(f, f.area(), &ctx, &SensorView::default())
        });
        assert!(out.contains("Fans 0 / 0 rpm"), "{out}");
    }

    #[test]
    fn wide_labels_keep_values_right_aligned() {
        let (mut s, h) = fixture::demo();
        s.sensors[0].readings = vec![("温度センサー".into(), 45.0)];
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: false,
            zoomed: false,
            idx: 7,
        };
        let mut v = SensorView::default();
        v.expanded.insert(SensorKind::Cpu);
        let out = draw(40, 12, |f| render(f, f.area(), &ctx, &v));
        let row = out.lines().find(|l| l.contains("温")).expect(&out);
        assert!(row.ends_with("45°C│"), "{row:?}");
    }
}

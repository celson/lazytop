use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::Ctx;
use crate::model::EngineState;
use crate::ui::{fmt, theme::Theme, widgets::panel_block};

fn dot(state: EngineState, t: &Theme) -> Span<'static> {
    let color = match state {
        EngineState::Active => t.accent,
        EngineState::Elevated => t.warn,
        _ => t.dim,
    };
    Span::styled("● ", Style::default().fg(color))
}

fn row(label: &str, state: EngineState, right: String, width: u16, t: &Theme) -> Line<'static> {
    let left = format!("{label:<6}");
    let text = state.label().to_string();
    let used = 6 + 2 + text.len();
    let pad = (width as usize).saturating_sub(used + fmt::width(&right));
    let style = if state == EngineState::Active {
        Style::default().fg(t.accent)
    } else {
        Style::default().fg(t.fg)
    };
    Line::from(vec![
        Span::styled(left, Style::default().fg(t.dim)),
        dot(state, t),
        Span::styled(text, style),
        Span::raw(" ".repeat(pad)),
        Span::styled(right, Style::default().fg(t.dim)),
    ])
}

pub fn render_workload(f: &mut Frame, area: Rect, c: &Ctx) {
    let t = c.theme;
    let s = c.snap;
    let block = panel_block("AI workload", Some(c.idx), c.focused, t);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width;
    let overall = if s.ai.busy {
        EngineState::Active
    } else {
        EngineState::Idle
    };
    let top =
        s.ai.top_proc
            .as_ref()
            .map(|(n, cpu)| format!("top: {} {:.0}%", fmt::truncate(n, 24), cpu))
            .unwrap_or_default();
    let gpu_right = match (&s.gpu, s.power.as_ref().and_then(|p| p.gpu_w)) {
        (Some(g), w) => format!("{} · {}", fmt::pct(g.usage), fmt::watts(w)),
        (None, _) => fmt::dash().into(),
    };
    let ane_right = fmt::watts(s.power.as_ref().and_then(|p| p.ane_w).filter(|w| *w > 0.0));
    let media_right = s
        .bandwidth
        .as_ref()
        .map(|b| format!("{:.1} GB/s", b.media / 1e9))
        .unwrap_or_else(|| fmt::dash().into());
    let lines = vec![
        Line::from(vec![
            dot(overall, t),
            Span::styled(
                if s.ai.busy { "Busy" } else { "Idle" },
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        row("CPU", s.ai.engines.cpu, top, w, t),
        row("GPU", s.ai.engines.gpu, gpu_right, w, t),
        row("ANE", s.ai.engines.ane, ane_right, w, t),
        row("Media", s.ai.engines.media, media_right, w, t),
        row("Mem", s.ai.engines.mem, String::new(), w, t),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

pub fn render_runtime(f: &mut Frame, area: Rect, c: &Ctx) {
    let t = c.theme;
    let s = c.snap;
    let block = panel_block("AI runtime", Some(c.idx), c.focused, t);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines = Vec::new();
    if s.ai.runtimes.is_empty() {
        lines.push(Line::styled(
            "No local AI runtime detected",
            Style::default().fg(t.fg),
        ));
    } else {
        for r in &s.ai.runtimes {
            lines.push(Line::from(vec![
                Span::styled(
                    "● ",
                    Style::default().fg(if r.cpu > 10.0 { t.accent } else { t.ok }),
                ),
                Span::styled(
                    fmt::truncate(&r.name, 28),
                    Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  pid {}  {:.0}%  {}", r.pid, r.cpu, fmt::bytes(r.mem)),
                    Style::default().fg(t.dim),
                ),
            ]));
        }
    }
    let budget = match s.ai.budget_b {
        Some(b) => format!("largest model ~{b}B (Q4_K_M)"),
        None => "not enough free memory (Q4_K_M)".into(),
    };
    lines.push(Line::from(vec![
        Span::styled("Model budget ", Style::default().fg(t.dim)),
        Span::styled(
            budget,
            Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
        ),
    ]));
    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{panels::fixture, test_util::render as draw};

    #[test]
    fn workload_rows() {
        let (s, h) = fixture::demo();
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: false,
            zoomed: false,
            idx: 1,
        };
        let out = draw(70, 8, |f| render_workload(f, f.area(), &ctx));
        for k in ["AI WORKLOAD", "CPU", "GPU", "ANE", "Media", "Mem", "top:"] {
            assert!(out.contains(k), "missing {k}\n{out}");
        }
    }

    #[test]
    fn runtime_detected_and_budget() {
        let (s, h) = fixture::demo();
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: false,
            zoomed: false,
            idx: 2,
        };
        let out = draw(70, 8, |f| render_runtime(f, f.area(), &ctx));
        assert!(out.contains("ollama"), "{out}");
        assert!(out.contains("Model budget"));
        assert!(out.contains("(Q4_K_M)"));
    }

    #[test]
    fn runtime_none() {
        let (mut s, h) = fixture::demo();
        s.ai.runtimes.clear();
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: false,
            zoomed: false,
            idx: 2,
        };
        let out = draw(70, 8, |f| render_runtime(f, f.area(), &ctx));
        assert!(out.contains("No local AI runtime detected"));
    }
}

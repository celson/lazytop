use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::model::Snapshot;
use crate::ui::{fmt, theme::Theme};

pub fn render(f: &mut Frame, area: Rect, s: &Snapshot, t: &Theme, paused: bool, interval_ms: u64) {
    let cores = match (s.host.cores_e, s.host.cores_p) {
        (Some(e), Some(p)) => format!("{} cores · {e}E+{p}P", s.host.cores_total),
        _ => format!("{} cores", s.host.cores_total),
    };
    let left = Line::from(vec![
        Span::styled(
            " lazytop ",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{}  ", s.host.chip), Style::default().fg(t.fg)),
        Span::styled(cores, Style::default().fg(t.dim)),
    ]);
    let mut right = vec![];
    if paused {
        right.push(Span::styled(
            "PAUSED  ",
            Style::default().fg(t.warn).add_modifier(Modifier::BOLD),
        ));
    }
    right.push(Span::styled("system ", Style::default().fg(t.dim)));
    right.push(Span::styled(
        fmt::watts(s.power.as_ref().and_then(|p| p.system_w)),
        Style::default().fg(t.fg),
    ));
    right.push(Span::styled(
        format!("  up {}  {}ms ", fmt::uptime(s.host.uptime), interval_ms),
        Style::default().fg(t.dim),
    ));
    f.render_widget(Paragraph::new(left).style(Style::default().bg(t.bg)), area);
    f.render_widget(
        Paragraph::new(Line::from(right)).alignment(ratatui::layout::Alignment::Right),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{panels::fixture, test_util::render as draw};

    #[test]
    fn shows_chip_cores_power() {
        let (s, _) = fixture::demo();
        let t = Theme::dark();
        let out = draw(120, 1, |f| render(f, f.area(), &s, &t, false, 1000));
        assert!(out.contains("lazytop"));
        assert!(out.contains("Apple M3 Pro"));
        assert!(out.contains("11 cores · 6E+5P"));
        assert!(out.contains(" W"));
    }

    #[test]
    fn shows_paused() {
        let (s, _) = fixture::demo();
        let out = draw(120, 1, |f| {
            render(f, f.area(), &s, &Theme::dark(), true, 1000)
        });
        assert!(out.contains("PAUSED"));
    }
}

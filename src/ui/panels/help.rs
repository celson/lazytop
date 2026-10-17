use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use crate::ui::{theme::Theme, widgets::panel_block};

const KEYS: [(&str, &str); 18] = [
    ("Tab / Shift-Tab, 1-8", "focus panel"),
    ("z / Enter", "zoom panel"),
    ("Esc", "unzoom / clear filter / close"),
    ("p", "pause sampling"),
    ("+ / -", "faster / slower refresh"),
    ("?", "toggle this help"),
    ("q / Ctrl-C", "quit"),
    ("", ""),
    ("Sensors: ↑↓ / Space", "move / expand group"),
    ("Processes: ↑↓ / j", "move selection"),
    ("PgUp / PgDn, g / G", "page / top / bottom"),
    ("/", "filter by name"),
    ("s / S", "cycle sort column / reverse"),
    ("t", "tree view"),
    ("k", "send SIGTERM (confirm)"),
    ("K", "send SIGKILL (confirm)"),
    ("Enter (zoomed)", "process details"),
    ("", ""),
];

pub fn render(f: &mut Frame, area: Rect, t: &Theme, notices: &[String]) {
    f.render_widget(Clear, area);
    let block = panel_block("Help", None, true, t);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines: Vec<Line> = KEYS
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(
                    format!("{k:<24}"),
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(d.to_string(), Style::default().fg(t.fg)),
            ])
        })
        .collect();
    if !notices.is_empty() {
        lines.push(Line::styled(
            "Unavailable metrics",
            Style::default().fg(t.dim).add_modifier(Modifier::BOLD),
        ));
        for n in notices {
            lines.push(Line::styled(format!("• {n}"), Style::default().fg(t.warn)));
        }
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::render as draw;

    #[test]
    fn lists_keys_and_notices() {
        let out = draw(80, 30, |f| {
            render(
                f,
                f.area(),
                &Theme::dark(),
                &["power: RAPL not readable".into()],
            )
        });
        for k in [
            "Tab",
            "zoom",
            "filter",
            "SIGTERM",
            "quit",
            "RAPL not readable",
        ] {
            assert!(out.contains(k), "missing {k}\n{out}");
        }
    }
}

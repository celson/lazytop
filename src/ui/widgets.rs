use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, BorderType, Borders, Chart, Dataset, GraphType, Widget};

use super::theme::Theme;

/// Two rows: `label ........ value` and a thin bar filled to `ratio`.
pub struct LabeledBar<'a> {
    pub label: &'a str,
    pub value: String,
    pub ratio: f64,
    pub color: Color,
    pub theme: &'a Theme,
}

impl Widget for LabeledBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let w = area.width as usize;
        let value_w = unicode_width::UnicodeWidthStr::width(self.value.as_str());
        let label = super::fmt::truncate(self.label, w.saturating_sub(value_w + 1));
        buf.set_string(area.x, area.y, &label, Style::default().fg(self.theme.dim));
        let vx = area.x + (w.saturating_sub(value_w)) as u16;
        buf.set_string(
            vx,
            area.y,
            &self.value,
            Style::default()
                .fg(self.theme.fg)
                .add_modifier(Modifier::BOLD),
        );
        if area.height < 2 {
            return;
        }
        let filled = ((self.ratio.clamp(0.0, 1.0) * w as f64).round() as usize).min(w);
        for i in 0..w {
            let color = if i < filled {
                self.color
            } else {
                self.theme.bar_bg
            };
            buf.set_string(
                area.x + i as u16,
                area.y + 1,
                "━",
                Style::default().fg(color),
            );
        }
    }
}

pub fn panel_block(
    title: &str,
    idx: Option<usize>,
    focused: bool,
    theme: &Theme,
) -> Block<'static> {
    let mut spans = Vec::new();
    if let Some(i) = idx {
        spans.push(Span::styled(
            format!("[{i}] "),
            Style::default().fg(if focused { theme.accent } else { theme.dim }),
        ));
    }
    spans.push(Span::styled(
        title.to_uppercase(),
        Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
    ));
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused {
            theme.border_focus
        } else {
            theme.border
        }))
        .title(Line::from(spans))
        .style(Style::default().bg(theme.bg).fg(theme.fg))
}

/// Renders a braille history chart for `series` into `area`.
pub fn render_history(
    frame: &mut ratatui::Frame,
    area: Rect,
    series: &[(Vec<(f64, f64)>, Color)],
    x_len: f64,
    y_max: f64,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let datasets: Vec<Dataset> = series
        .iter()
        .map(|(pts, color)| {
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(*color))
                .data(pts)
        })
        .collect();
    let chart = Chart::new(datasets)
        .x_axis(Axis::default().bounds([0.0, x_len.max(1.0)]))
        .y_axis(Axis::default().bounds([0.0, y_max.max(f64::EPSILON)]));
    frame.render_widget(chart, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_util::render;

    #[test]
    fn labeled_bar_renders_label_value_and_fill() {
        let t = Theme::dark();
        let out = render(20, 2, |f| {
            f.render_widget(
                LabeledBar {
                    label: "Efficiency",
                    value: "13%".into(),
                    ratio: 0.5,
                    color: t.e_core,
                    theme: &t,
                },
                f.area(),
            );
        });
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[0].starts_with("Efficiency"));
        assert!(lines[0].ends_with("13%"));
        assert_eq!(lines[1].chars().filter(|c| *c == '━').count(), 20);
    }

    #[test]
    fn panel_block_title() {
        let t = Theme::dark();
        let out = render(20, 3, |f| {
            f.render_widget(panel_block("cpu", Some(3), true, &t), f.area())
        });
        assert!(out.lines().next().unwrap().contains("[3] CPU"));
    }

    #[test]
    fn history_handles_empty_and_tiny_area() {
        let t = Theme::dark();
        let _ = render(1, 1, |f| {
            render_history(f, f.area(), &[(vec![], t.gpu)], 0.0, 0.0)
        });
        let _ = render(10, 3, |f| {
            render_history(
                f,
                f.area(),
                &[(vec![(0.0, 1.0), (1.0, 0.5)], t.gpu)],
                1.0,
                1.0,
            )
        });
    }
}

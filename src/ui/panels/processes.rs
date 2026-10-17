use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row as TRow, Table};

use super::Ctx;
use crate::config::SortKey;
use crate::ui::process::{ProcView, arrange};
use crate::ui::{fmt, widgets::panel_block};

pub fn render(
    f: &mut Frame,
    area: Rect,
    c: &Ctx,
    v: &mut ProcView,
    filter_editing: bool,
    details: bool,
) {
    let t = c.theme;
    let procs = &c.snap.procs;
    let block = panel_block("Processes", Some(c.idx), c.focused, t);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = arrange(procs, &v.filter, v.sort, v.reverse, v.tree);
    v.sync(&rows, procs);

    let detail_h = if details && c.zoomed { 7 } else { 0 };
    let [filter_area, table_area, detail_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(detail_h),
    ])
    .areas(inner);

    let filter_line = if filter_editing || !v.filter.is_empty() {
        Line::from(vec![
            Span::styled(format!("/{}", v.filter), Style::default().fg(t.fg)),
            Span::styled(
                if filter_editing { "█" } else { "" },
                Style::default().fg(t.accent),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled("⌕ Filter by name", Style::default().fg(t.dim)),
            Span::styled(
                format!(
                    "   sort: {}{}  {}",
                    v.sort.label(),
                    if v.reverse { "▴" } else { "▾" },
                    if v.tree { "tree" } else { "" }
                ),
                Style::default().fg(t.dim),
            ),
        ])
    };
    f.render_widget(Paragraph::new(filter_line), filter_area);

    if rows.is_empty() {
        f.render_widget(
            Paragraph::new(Line::styled(
                "no matching processes",
                Style::default().fg(t.dim),
            )),
            table_area,
        );
        return;
    }

    let visible = table_area.height.saturating_sub(1) as usize;
    v.page = visible;
    if v.cursor < v.offset {
        v.offset = v.cursor;
    } else if visible > 0 && v.cursor >= v.offset + visible {
        v.offset = v.cursor + 1 - visible;
    }

    let arrow = if v.reverse { "▴" } else { "▾" };
    let head = |key: SortKey, label: &str| {
        let style = if v.sort == key {
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.dim)
        };
        Cell::from(Span::styled(
            if v.sort == key {
                format!("{label}{arrow}")
            } else {
                label.to_string()
            },
            style,
        ))
    };
    let mut header_cells = vec![
        head(SortKey::Pid, "PID"),
        head(SortKey::Cpu, "CPU%"),
        head(SortKey::Mem, "MEMORY"),
    ];
    if c.zoomed {
        header_cells.push(Cell::from(Span::styled("USER", Style::default().fg(t.dim))));
    }
    header_cells.push(head(SortKey::Name, "NAME"));
    let name_w = table_area
        .width
        .saturating_sub(if c.zoomed { 41 } else { 29 }) as usize;

    let body: Vec<TRow> = rows
        .iter()
        .enumerate()
        .skip(v.offset)
        .take(visible)
        .map(|(i, r)| {
            let p = &procs[r.idx];
            let indent = if v.tree && r.depth > 0 {
                format!("{}└ ", "  ".repeat(r.depth - 1))
            } else {
                String::new()
            };
            let mut cells = vec![
                Cell::from(Span::styled(
                    fmt::thousands(p.pid),
                    Style::default().fg(t.dim),
                )),
                Cell::from(format!("{:>5.1}", p.cpu)),
                Cell::from(format!("{:>7}", fmt::bytes(p.mem))),
            ];
            if c.zoomed {
                cells.push(Cell::from(fmt::truncate(&p.user, 10)));
            }
            cells.push(Cell::from(fmt::truncate(
                &format!("{indent}{}", p.name),
                name_w,
            )));
            let style = if i == v.cursor && c.focused {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            TRow::new(cells).style(style)
        })
        .collect();
    // PID column fits "4.194.303" (Linux pid_max with thousands separators).
    let mut widths = vec![
        Constraint::Length(9),
        Constraint::Length(6),
        Constraint::Length(8),
    ];
    if c.zoomed {
        widths.push(Constraint::Length(10));
    }
    widths.push(Constraint::Fill(1));
    f.render_widget(
        Table::new(body, widths)
            .header(TRow::new(header_cells))
            .column_spacing(1),
        table_area,
    );

    if detail_h > 0
        && let Some(p) = v.selected(&rows, procs)
    {
        let kv = |k: &str, val: String| {
            Line::from(vec![
                Span::styled(format!("{k:<9}"), Style::default().fg(t.dim)),
                Span::raw(val),
            ])
        };
        let lines = vec![
            kv(
                "Command",
                fmt::truncate(&p.cmd, detail_area.width.saturating_sub(10) as usize),
            ),
            kv("User", p.user.clone()),
            kv(
                "PID",
                format!(
                    "{} (parent {})",
                    p.pid,
                    p.ppid
                        .map(|x| x.to_string())
                        .unwrap_or_else(|| fmt::dash().into())
                ),
            ),
            kv(
                "Threads",
                p.threads
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| fmt::dash().into()),
            ),
            kv("Memory", fmt::bytes(p.mem)),
            kv("Started", format!("{} s since epoch", p.start_time)),
        ];
        f.render_widget(Paragraph::new(lines), detail_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProcCfg;
    use crate::ui::{panels::fixture, test_util::render as draw, theme::Theme};

    #[test]
    fn table_with_header_and_rows() {
        let (s, h) = fixture::demo();
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: true,
            zoomed: false,
            idx: 8,
        };
        let mut v = ProcView::from_cfg(&ProcCfg::default());
        let out = draw(80, 14, |f| render(f, f.area(), &ctx, &mut v, false, false));
        assert!(
            out.contains("PID")
                && out.contains("CPU%▾")
                && out.contains("MEMORY")
                && out.contains("NAME")
        );
        assert!(out.contains("Filter by name"));
        assert!(v.selected_pid.is_some());
    }

    #[test]
    fn filter_no_match_shows_message() {
        let (s, h) = fixture::demo();
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: true,
            zoomed: false,
            idx: 8,
        };
        let mut v = ProcView::from_cfg(&ProcCfg::default());
        v.filter = "zzzz".into();
        let out = draw(80, 14, |f| render(f, f.area(), &ctx, &mut v, true, false));
        assert!(out.contains("/zzzz"));
        assert!(out.contains("no matching processes"));
    }

    #[test]
    fn details_view() {
        let (s, h) = fixture::demo();
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: true,
            zoomed: true,
            idx: 8,
        };
        let mut v = ProcView::from_cfg(&ProcCfg::default());
        let out = draw(120, 30, |f| render(f, f.area(), &ctx, &mut v, false, true));
        assert!(out.contains("Command") && out.contains("User") && out.contains("Threads"));
    }

    #[test]
    fn large_linux_pids_are_not_truncated() {
        let (mut s, h) = fixture::demo();
        s.procs[0].pid = 4_194_303;
        s.procs[0].cpu = 99.0;
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: true,
            zoomed: false,
            idx: 8,
        };
        let mut v = ProcView::from_cfg(&ProcCfg::default());
        let out = draw(80, 10, |f| render(f, f.area(), &ctx, &mut v, false, false));
        assert!(out.contains("4.194.303"), "{out}");
    }
}

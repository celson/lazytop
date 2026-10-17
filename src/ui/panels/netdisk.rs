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

fn kv(label: &str, value: String, color: Color, width: u16, t: &Theme) -> Line<'static> {
    let pad = fmt::pad(label, &value, width as usize);
    Line::from(vec![
        Span::styled(label.to_string(), Style::default().fg(t.dim)),
        Span::raw(" ".repeat(pad)),
        Span::styled(value, Style::default().fg(color)),
    ])
}

fn heading(s: &str, t: &Theme) -> Line<'static> {
    Line::styled(
        s.to_string(),
        Style::default().fg(t.dim).add_modifier(Modifier::BOLD),
    )
}

pub fn render(f: &mut Frame, area: Rect, c: &Ctx) {
    let t = c.theme;
    let s = c.snap;
    let block = panel_block("Network & disk", Some(c.idx), c.focused, t);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .spacing(2)
            .areas(inner);

    let w = left.width;
    let mut net = vec![
        heading("NETWORK", t),
        kv("↓ Download", fmt::rate(s.net.rx), t.rx, w, t),
        kv("↑ Upload", fmt::rate(s.net.tx), t.tx, w, t),
    ];
    let max_ifaces = if c.zoomed { usize::MAX } else { 3 };
    for i in s.net.ifaces.iter().take(max_ifaces) {
        net.push(kv(
            &format!("  {}", fmt::truncate(&i.name, 12)),
            format!("↓ {}  ↑ {}", fmt::rate(i.rx), fmt::rate(i.tx)),
            t.fg,
            w,
            t,
        ));
    }
    let n = net.len() as u16;
    let [ntxt, nchart] = Layout::vertical([Constraint::Length(n), Constraint::Fill(1)]).areas(left);
    f.render_widget(Paragraph::new(net), ntxt);
    let nmax = c.hist.max(Series::NetRx).max(c.hist.max(Series::NetTx));
    render_history(
        f,
        nchart,
        &[
            (c.hist.points(Series::NetRx), t.rx),
            (c.hist.points(Series::NetTx), t.tx),
        ],
        c.hist.capacity() as f64,
        nmax * 1.1,
    );

    let w = right.width;
    let d = &s.disk;
    let rate = |v: Option<f64>| v.map(fmt::rate).unwrap_or_else(|| fmt::dash().into());
    let top = vec![
        heading("DISK", t),
        kv("Read", rate(d.read), t.read, w, t),
        kv("Write", rate(d.write), t.write, w, t),
    ];
    let mut boot = vec![heading("SINCE BOOT", t)];
    boot.push(kv(
        "Read",
        d.read_total
            .map(fmt::bytes)
            .unwrap_or_else(|| fmt::dash().into()),
        t.read,
        w,
        t,
    ));
    boot.push(kv(
        "Write",
        d.write_total
            .map(fmt::bytes)
            .unwrap_or_else(|| fmt::dash().into()),
        t.write,
        w,
        t,
    ));
    let [a, b, cc, chart] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .areas(right);
    f.render_widget(Paragraph::new(top), a);
    f.render_widget(
        LabeledBar {
            label: "Used",
            value: format!("{} / {}", fmt::bytes(d.used), fmt::bytes(d.total)),
            ratio: d.used as f64 / d.total.max(1) as f64,
            color: t.dim,
            theme: t,
        },
        b,
    );
    f.render_widget(Paragraph::new(boot), cc);
    let dmax = c
        .hist
        .max(Series::DiskRead)
        .max(c.hist.max(Series::DiskWrite));
    render_history(
        f,
        chart,
        &[
            (c.hist.points(Series::DiskRead), t.read),
            (c.hist.points(Series::DiskWrite), t.write),
        ],
        c.hist.capacity() as f64,
        dmax * 1.1,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{panels::fixture, test_util::render as draw};

    #[test]
    fn net_and_disk() {
        let (s, h) = fixture::demo();
        let t = Theme::dark();
        let ctx = Ctx {
            snap: &s,
            hist: &h,
            theme: &t,
            focused: false,
            zoomed: false,
            idx: 6,
        };
        let out = draw(80, 16, |f| render(f, f.area(), &ctx));
        for k in [
            "Download",
            "Upload",
            "en0",
            "Read",
            "Write",
            "Used",
            "SINCE BOOT",
            "KB/s",
        ] {
            assert!(out.contains(k), "missing {k}\n{out}");
        }
    }
}

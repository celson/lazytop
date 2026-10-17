pub mod app;
pub mod fmt;
pub mod layout;
pub mod panels;
pub mod process;
pub mod theme;
pub mod widgets;

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::{Frame, Terminal, backend::TestBackend};

use app::{App, Mode};
use layout::PanelId;
use panels::Ctx;

use crate::model::Snapshot;

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let t = app.theme.clone();
    f.render_widget(
        Block::default().style(Style::default().bg(t.bg).fg(t.fg)),
        area,
    );
    if area.width < layout::MIN_W || area.height < layout::MIN_H {
        f.render_widget(Paragraph::new("terminal too small"), area);
        return;
    }
    let Some(snap) = app.snap.clone() else {
        f.render_widget(
            Paragraph::new("collecting first sample…"),
            centered(area, 30, 1),
        );
        return;
    };
    let [header, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);
    panels::header::render(f, header, &snap, &t, app.paused, app.interval_ms);

    let visible = app.visible();
    let zoom = app.zoomed.then_some(app.focus);
    for (id, r) in layout::compute(body, &visible, zoom, app.focus) {
        let ctx = Ctx {
            snap: &snap,
            hist: &app.hist,
            theme: &t,
            focused: id == app.focus,
            zoomed: app.zoomed,
            idx: id.number(),
        };
        match id {
            PanelId::AiWorkload => panels::ai::render_workload(f, r, &ctx),
            PanelId::AiRuntime => panels::ai::render_runtime(f, r, &ctx),
            PanelId::Cpu => panels::cpu::render(f, r, &ctx),
            PanelId::Gpu => panels::gpu::render(f, r, &ctx),
            PanelId::Memory => panels::memory::render(f, r, &ctx),
            PanelId::NetDisk => panels::netdisk::render(f, r, &ctx),
            PanelId::Sensors => panels::sensors::render(f, r, &ctx, &app.sensors),
            PanelId::Processes => panels::processes::render(
                f,
                r,
                &ctx,
                &mut app.procs,
                app.mode == Mode::Filter,
                app.details,
            ),
        }
    }

    let hints = match (&app.mode, app.focus) {
        (Mode::Filter, _) => "type to filter · Enter keep · Esc clear",
        (Mode::Confirm(..), _) => "y confirm · any other key cancel",
        (_, PanelId::Processes) => {
            "Tab/1-8 focus · z zoom · / filter · s sort · t tree · k/K kill · p pause · ? help · q quit"
        }
        (_, PanelId::Sensors) => {
            "Tab/1-8 focus · z zoom · ↑↓ select · Space expand · p pause · ? help · q quit"
        }
        _ => "Tab/1-8 focus · z zoom · +/- interval · p pause · ? help · q quit",
    };
    let line = match app.status_text() {
        Some(msg) => Line::from(Span::styled(
            format!(" {msg}"),
            Style::default().fg(t.warn).add_modifier(Modifier::BOLD),
        )),
        None => Line::from(Span::styled(
            format!(" {hints}"),
            Style::default().fg(t.dim),
        )),
    };
    f.render_widget(Paragraph::new(line), status);

    match &app.mode {
        Mode::Help => panels::help::render(f, centered(area, 70, 28), &t, &app.notices),
        Mode::Confirm(pid, name, sig) => {
            let r = centered(area, 50, 5);
            f.render_widget(Clear, r);
            let block = widgets::panel_block("Confirm", None, true, &t);
            let inner = block.inner(r);
            f.render_widget(block, r);
            let sig = if *sig == process::Sig::Kill {
                "SIGKILL"
            } else {
                "SIGTERM"
            };
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(format!(
                        "Send {sig} to {pid} ({})?",
                        fmt::truncate(name, 24)
                    )),
                    Line::styled("y / Enter = yes · any key = no", Style::default().fg(t.dim)),
                ]),
                inner,
            );
        }
        _ => {}
    }
}

/// Renders one frame to text (used by `--dump` and tests).
pub fn dump(app: &mut App, w: u16, h: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).expect("test backend");
    term.draw(|f| draw(f, app)).expect("draw");
    let buf = term.backend().buffer().clone();
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Applies every pending snapshot. `Ok(true)` if any arrived; an error once the collector thread
/// is gone (e.g. it panicked), so the UI stops instead of showing frozen data.
pub fn drain(rx: &Receiver<Snapshot>, app: &mut App) -> anyhow::Result<bool> {
    let mut got = false;
    loop {
        match rx.try_recv() {
            Ok(s) => {
                app.on_snapshot(s);
                got = true;
            }
            Err(TryRecvError::Empty) => return Ok(got),
            Err(TryRecvError::Disconnected) => anyhow::bail!("collector stopped unexpectedly"),
        }
    }
}

pub fn run(mut app: App, rx: Receiver<Snapshot>) -> anyhow::Result<()> {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "an interactive terminal is required (use --dump WIDTHxHEIGHT for text output)"
        );
    }
    // try_init enables raw mode + alternate screen and installs a panic hook that restores the terminal.
    let mut terminal =
        ratatui::try_init().map_err(|e| anyhow::anyhow!("failed to initialize terminal: {e}"))?;
    let res = (|| -> anyhow::Result<()> {
        // Redraw only when something changed: a new snapshot, input, resize, or the status line expiring.
        let mut dirty = true;
        let mut status_shown = false;
        loop {
            if drain(&rx, &mut app)? {
                dirty = true;
            }
            if status_shown != app.status_text().is_some() {
                dirty = true;
            }
            if dirty {
                terminal.draw(|f| draw(f, &mut app))?;
                status_shown = app.status_text().is_some();
                dirty = false;
            }
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => app.on_key(k),
                    Event::Resize(..) => {}
                    _ => continue,
                }
                dirty = true;
            }
            if app.quit {
                return Ok(());
            }
        }
    })();
    ratatui::restore();
    res
}

#[cfg(test)]
pub mod test_util {
    use ratatui::{Frame, Terminal, backend::TestBackend};

    /// Renders into a TestBackend and returns the screen as lines joined by '\n'.
    pub fn render(w: u16, h: u16, f: impl FnOnce(&mut Frame)) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(f).unwrap();
        let buf = t.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::{Collector, demo::DemoCollector};
    use crate::config::Config;
    use crate::ui::theme::Theme;

    fn app() -> App {
        let mut a = App::new(
            Config::default(),
            Theme::dark(),
            None,
            Box::new(|_, _| Ok(())),
        );
        let mut c = DemoCollector::new(9);
        for _ in 0..10 {
            a.on_snapshot(c.sample());
        }
        a
    }

    #[test]
    fn full_dashboard_wide_and_narrow() {
        let mut a = app();
        let wide = dump(&mut a, 160, 48);
        for k in [
            "lazytop",
            "[1] AI WORKLOAD",
            "[3] CPU",
            "[4] GPU",
            "[5] MEMORY",
            "[6] NETWORK",
            "[7] SENSORS",
            "[8] PROCESSES",
        ] {
            assert!(wide.contains(k), "missing {k}\n{wide}");
        }
        let narrow = dump(&mut a, 80, 24);
        assert!(narrow.contains("lazytop"));
    }

    #[test]
    fn tiny_terminal_does_not_panic() {
        let mut a = app();
        assert!(dump(&mut a, 30, 8).contains("terminal too small"));
        let _ = dump(&mut a, 1, 1);
    }

    #[test]
    fn help_and_confirm_overlays() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut a = app();
        a.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        assert!(dump(&mut a, 160, 48).contains("HELP"));
        a.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        a.on_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert!(dump(&mut a, 160, 48).contains("Send SIGTERM"));
    }

    #[test]
    fn zoomed_panel_fills_body() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut a = app();
        a.on_key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));
        a.on_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
        let out = dump(&mut a, 160, 48);
        assert!(out.contains("core 10"));
        assert!(!out.contains("[8] PROCESSES"));
    }

    #[test]
    fn drain_errors_when_collector_is_gone() {
        let mut a = app();
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(DemoCollector::new(2).sample()).unwrap();
        assert!(drain(&rx, &mut a).unwrap());
        assert!(!drain(&rx, &mut a).unwrap());
        drop(tx);
        assert!(drain(&rx, &mut a).is_err());
    }
}

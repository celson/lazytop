use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::layout::PanelId;
use super::panels::{SensorView, gpu_has_data, sensors_have_data};
use super::process::{ProcView, Sig, arrange};
use super::theme::Theme;
use crate::collector::Control;
use crate::config::{self, Config};
use crate::history::History;
use crate::model::Snapshot;

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Filter,
    /// pid, name, signal
    Confirm(u32, String, Sig),
    Help,
}

pub type Killer = Box<dyn Fn(u32, Sig) -> Result<(), String>>;

pub struct App {
    pub cfg: Config,
    pub theme: Theme,
    pub snap: Option<Snapshot>,
    pub hist: History,
    pub focus: PanelId,
    pub zoomed: bool,
    pub details: bool,
    pub mode: Mode,
    pub procs: ProcView,
    pub sensors: SensorView,
    pub status: Option<(String, Instant)>,
    pub paused: bool,
    pub interval_ms: u64,
    pub quit: bool,
    pub notices: Vec<String>,
    ctl: Option<Sender<Control>>,
    killer: Killer,
}

const STATUS_TTL: Duration = Duration::from_secs(3);

impl App {
    pub fn new(cfg: Config, theme: Theme, ctl: Option<Sender<Control>>, killer: Killer) -> App {
        App {
            procs: ProcView::from_cfg(&cfg.processes),
            interval_ms: cfg.interval_ms,
            cfg,
            theme,
            snap: None,
            hist: History::new(300),
            focus: PanelId::Processes,
            zoomed: false,
            details: false,
            mode: Mode::Normal,
            sensors: SensorView::default(),
            status: None,
            paused: false,
            quit: false,
            notices: Vec::new(),
            ctl,
            killer,
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now()));
    }

    pub fn status_text(&self) -> Option<&str> {
        self.status
            .as_ref()
            .filter(|(_, t)| t.elapsed() < STATUS_TTL)
            .map(|(m, _)| m.as_str())
    }

    pub fn on_snapshot(&mut self, s: Snapshot) {
        let first = self.snap.is_none();
        if first
            && s.gpu.is_none()
            && s.power.is_none()
            && s.bandwidth.is_none()
            && s.sensors.is_empty()
        {
            self.set_status("platform metrics unavailable (sysinfo only) — press ? for details");
        }
        self.hist.push(&s);
        self.notices = notices_for(&s);
        self.snap = Some(s);
        if !self.visible().contains(&self.focus) {
            self.focus = PanelId::Processes;
        }
    }

    pub fn visible(&self) -> Vec<PanelId> {
        PanelId::ALL
            .into_iter()
            .filter(|p| !self.cfg.panels.hidden.iter().any(|h| h == p.config_name()))
            .filter(|p| {
                let Some(s) = &self.snap else { return true };
                if !self.cfg.panels.hide_empty {
                    return true;
                }
                match p {
                    PanelId::Gpu => gpu_has_data(s),
                    PanelId::Sensors => sensors_have_data(s),
                    _ => true,
                }
            })
            .collect()
    }

    fn send(&self, c: Control) {
        if let Some(tx) = &self.ctl {
            let _ = tx.send(c);
        }
    }

    fn cycle_focus(&mut self, forward: bool) {
        let v = self.visible();
        let Some(pos) = v.iter().position(|p| *p == self.focus) else {
            self.focus = v.first().copied().unwrap_or(PanelId::Processes);
            return;
        };
        let n = v.len();
        self.focus = v[if forward {
            (pos + 1) % n
        } else {
            (pos + n - 1) % n
        }];
    }

    fn set_interval(&mut self, ms: u64) {
        self.interval_ms = config::clamp_interval(ms);
        self.send(Control::SetInterval(Duration::from_millis(
            self.interval_ms,
        )));
        self.set_status(format!("refresh every {} ms", self.interval_ms));
    }

    fn proc_rows(&self) -> (Vec<super::process::Row>, Vec<crate::model::ProcInfo>) {
        let procs = self
            .snap
            .as_ref()
            .map(|s| s.procs.clone())
            .unwrap_or_default();
        let rows = arrange(
            &procs,
            &self.procs.filter,
            self.procs.sort,
            self.procs.reverse,
            self.procs.tree,
        );
        (rows, procs)
    }

    pub fn on_key(&mut self, k: KeyEvent) {
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match self.mode.clone() {
            Mode::Help => {
                if matches!(
                    k.code,
                    KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q')
                ) {
                    self.mode = Mode::Normal;
                }
            }
            Mode::Filter => match k.code {
                KeyCode::Esc => {
                    self.procs.filter.clear();
                    self.mode = Mode::Normal;
                }
                KeyCode::Enter => self.mode = Mode::Normal,
                KeyCode::Backspace => {
                    self.procs.filter.pop();
                }
                KeyCode::Char(c) => self.procs.filter.push(c),
                _ => {}
            },
            Mode::Confirm(pid, name, sig) => {
                if matches!(
                    k.code,
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter
                ) {
                    let label = if sig == Sig::Kill {
                        "SIGKILL"
                    } else {
                        "SIGTERM"
                    };
                    match (self.killer)(pid, sig) {
                        Ok(()) => self.set_status(format!("{label} sent to {pid} ({name})")),
                        Err(e) => self.set_status(e),
                    }
                }
                self.mode = Mode::Normal;
            }
            Mode::Normal => self.on_key_normal(k),
        }
    }

    fn on_key_normal(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Tab => self.cycle_focus(true),
            KeyCode::BackTab => self.cycle_focus(false),
            KeyCode::Char(c @ '1'..='8') => {
                if let Some(p) = PanelId::from_number(c as usize - '0' as usize)
                    && self.visible().contains(&p)
                {
                    self.focus = p;
                }
            }
            KeyCode::Char('z') => self.zoomed = !self.zoomed,
            KeyCode::Enter if self.focus == PanelId::Processes && self.zoomed => {
                self.details = !self.details
            }
            KeyCode::Enter => self.zoomed = !self.zoomed,
            KeyCode::Esc => {
                if self.details {
                    self.details = false;
                } else if self.zoomed {
                    self.zoomed = false;
                } else {
                    self.procs.filter.clear();
                }
            }
            KeyCode::Char('p') => {
                self.paused = !self.paused;
                self.send(Control::Pause(self.paused));
            }
            KeyCode::Char('+') | KeyCode::Char('=') => self.set_interval(self.interval_ms / 2),
            KeyCode::Char('-') => self.set_interval(self.interval_ms * 2),
            _ => match self.focus {
                PanelId::Processes => self.on_key_procs(k),
                PanelId::Sensors => self.on_key_sensors(k),
                _ => {}
            },
        }
    }

    fn on_key_procs(&mut self, k: KeyEvent) {
        let (rows, procs) = self.proc_rows();
        self.procs.sync(&rows, &procs);
        let page = self.procs.page.max(1) as isize;
        match k.code {
            KeyCode::Up => self.procs.move_by(-1, &rows, &procs),
            KeyCode::Down | KeyCode::Char('j') => self.procs.move_by(1, &rows, &procs),
            KeyCode::PageUp => self.procs.move_by(-page, &rows, &procs),
            KeyCode::PageDown => self.procs.move_by(page, &rows, &procs),
            KeyCode::Char('g') => self.procs.top(&rows, &procs),
            KeyCode::Char('G') => self.procs.bottom(&rows, &procs),
            KeyCode::Char('/') => self.mode = Mode::Filter,
            KeyCode::Char('s') => self.procs.sort = self.procs.sort.next(),
            KeyCode::Char('S') => self.procs.reverse = !self.procs.reverse,
            KeyCode::Char('t') => self.procs.tree = !self.procs.tree,
            KeyCode::Char(c @ ('k' | 'K')) => {
                if let Some(p) = self.procs.selected(&rows, &procs) {
                    let sig = if c == 'K' { Sig::Kill } else { Sig::Term };
                    self.mode = Mode::Confirm(p.pid, p.name.clone(), sig);
                }
            }
            _ => {}
        }
    }

    fn on_key_sensors(&mut self, k: KeyEvent) {
        let groups: Vec<_> = self
            .snap
            .as_ref()
            .map(|s| s.sensors.iter().map(|g| g.kind).collect())
            .unwrap_or_default();
        if groups.is_empty() {
            return;
        }
        let sel = &mut self.sensors.selected;
        match k.code {
            KeyCode::Up => *sel = sel.saturating_sub(1),
            KeyCode::Down => *sel = (*sel + 1).min(groups.len() - 1),
            KeyCode::Char(' ') => {
                let kind = groups[(*sel).min(groups.len() - 1)];
                if !self.sensors.expanded.remove(&kind) {
                    self.sensors.expanded.insert(kind);
                }
            }
            _ => {}
        }
    }
}

fn notices_for(s: &Snapshot) -> Vec<String> {
    let mut n = Vec::new();
    if s.power.is_none() {
        n.push(if cfg!(target_os = "linux") {
            "power: RAPL energy counters are not readable by this user (root-only on recent kernels)".into()
        } else {
            "power: IOReport energy channels unavailable".into()
        });
    }
    if s.bandwidth.is_none() {
        n.push("memory bandwidth: no supported counters on this machine".into());
    }
    if s.gpu.is_none() {
        n.push("GPU: no supported GPU found (Apple Silicon, NVIDIA NVML or AMD amdgpu)".into());
    }
    if s.sensors.is_empty() {
        n.push("sensors: no temperature sources found".into());
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::{Collector, demo::DemoCollector};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn app_with(killed: Rc<RefCell<Vec<(u32, Sig)>>>) -> App {
        let k = killed.clone();
        let mut a = App::new(
            Config::default(),
            Theme::dark(),
            None,
            Box::new(move |pid, sig| {
                k.borrow_mut().push((pid, sig));
                Ok(())
            }),
        );
        a.on_snapshot(DemoCollector::new(3).sample());
        a
    }

    fn app() -> App {
        app_with(Rc::new(RefCell::new(vec![])))
    }

    #[test]
    fn focus_cycles_and_numbers() {
        let mut a = app();
        assert_eq!(a.focus, PanelId::Processes);
        a.on_key(key(KeyCode::Tab));
        assert_eq!(a.focus, PanelId::AiWorkload);
        a.on_key(key(KeyCode::BackTab));
        assert_eq!(a.focus, PanelId::Processes);
        a.on_key(key(KeyCode::Char('3')));
        assert_eq!(a.focus, PanelId::Cpu);
    }

    #[test]
    fn zoom_and_escape() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('3')));
        a.on_key(key(KeyCode::Char('z')));
        assert!(a.zoomed);
        a.on_key(key(KeyCode::Esc));
        assert!(!a.zoomed);
    }

    #[test]
    fn interval_clamped_and_pause() {
        let mut a = app();
        for _ in 0..20 {
            a.on_key(key(KeyCode::Char('-')));
        }
        assert_eq!(a.interval_ms, config::MAX_INTERVAL_MS);
        for _ in 0..40 {
            a.on_key(key(KeyCode::Char('+')));
        }
        assert_eq!(a.interval_ms, config::MIN_INTERVAL_MS);
        a.on_key(key(KeyCode::Char('p')));
        assert!(a.paused);
    }

    #[test]
    fn filter_mode_edits_and_escape_clears() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('/')));
        assert_eq!(a.mode, Mode::Filter);
        for c in "oll".chars() {
            a.on_key(key(KeyCode::Char(c)));
        }
        // 'q' while filtering is text, not quit
        a.on_key(key(KeyCode::Char('q')));
        assert!(!a.quit);
        a.on_key(key(KeyCode::Backspace));
        assert_eq!(a.procs.filter, "oll");
        a.on_key(key(KeyCode::Enter));
        assert_eq!(a.mode, Mode::Normal);
        assert_eq!(a.procs.filter, "oll");
        a.on_key(key(KeyCode::Esc));
        assert_eq!(a.procs.filter, "");
    }

    #[test]
    fn kill_requires_confirmation() {
        let killed = Rc::new(RefCell::new(vec![]));
        let mut a = app_with(killed.clone());
        a.on_key(key(KeyCode::Char('k')));
        let Mode::Confirm(pid, _, Sig::Term) = a.mode.clone() else {
            panic!("{:?}", a.mode)
        };
        a.on_key(key(KeyCode::Char('n')));
        assert!(killed.borrow().is_empty());
        a.on_key(key(KeyCode::Char('K')));
        a.on_key(key(KeyCode::Char('y')));
        assert_eq!(killed.borrow().as_slice(), &[(pid, Sig::Kill)]);
        assert!(a.status.as_ref().unwrap().0.contains("SIGKILL"));
    }

    #[test]
    fn kill_on_empty_list_is_noop() {
        let mut a = app();
        a.procs.filter = "zzzz".into();
        a.on_key(key(KeyCode::Char('k')));
        assert_eq!(a.mode, Mode::Normal);
    }

    #[test]
    fn kill_error_goes_to_status() {
        let mut a = App::new(
            Config::default(),
            Theme::dark(),
            None,
            Box::new(|pid, _| Err(format!("no such process: {pid}"))),
        );
        a.on_snapshot(DemoCollector::new(3).sample());
        a.on_key(key(KeyCode::Char('k')));
        a.on_key(key(KeyCode::Char('y')));
        assert!(a.status.as_ref().unwrap().0.contains("no such process"));
    }

    #[test]
    fn sort_and_tree_keys() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('s')));
        assert_eq!(a.procs.sort, crate::config::SortKey::Mem);
        a.on_key(key(KeyCode::Char('S')));
        assert!(a.procs.reverse);
        a.on_key(key(KeyCode::Char('t')));
        assert!(a.procs.tree);
    }

    #[test]
    fn sensors_expand_with_space() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('7')));
        a.on_key(key(KeyCode::Char(' ')));
        assert!(a.sensors.expanded.contains(&crate::model::SensorKind::Cpu));
        a.on_key(key(KeyCode::Down));
        assert_eq!(a.sensors.selected, 1);
    }

    #[test]
    fn hidden_and_empty_panels() {
        let mut cfg = Config::default();
        cfg.panels.hidden = vec!["ai_runtime".into()];
        let mut a = App::new(cfg, Theme::dark(), None, Box::new(|_, _| Ok(())));
        let mut s = DemoCollector::new(1).sample();
        s.gpu = None;
        s.power = None;
        s.bandwidth = None;
        a.on_snapshot(s);
        let v = a.visible();
        assert!(!v.contains(&PanelId::AiRuntime));
        assert!(!v.contains(&PanelId::Gpu));
        // focus skips hidden panels
        a.on_key(key(KeyCode::Char('2')));
        assert_ne!(a.focus, PanelId::AiRuntime);
    }

    #[test]
    fn quit_keys() {
        let mut a = app();
        a.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(a.quit);
    }

    #[test]
    fn help_toggles() {
        let mut a = app();
        a.on_key(key(KeyCode::Char('?')));
        assert_eq!(a.mode, Mode::Help);
        a.on_key(key(KeyCode::Char('?')));
        assert_eq!(a.mode, Mode::Normal);
    }

    #[test]
    fn status_notice_when_no_platform_metrics() {
        let mut a = App::new(
            Config::default(),
            Theme::dark(),
            None,
            Box::new(|_, _| Ok(())),
        );
        let mut s = DemoCollector::new(1).sample();
        s.gpu = None;
        s.power = None;
        s.bandwidth = None;
        s.sensors.clear();
        s.fans.clear();
        a.on_snapshot(s);
        assert!(
            a.status_text().is_some_and(|m| m.contains("press ?")),
            "{:?}",
            a.status
        );
    }

    #[test]
    fn no_notice_with_platform_metrics() {
        let a = app();
        assert!(a.status_text().is_none());
    }
}

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use lazytop::collector::{self, Collector, demo::DemoCollector};
use lazytop::config::{self, Config};
use lazytop::ui::{self, app::App, process, theme::Theme};

#[derive(Parser, Debug)]
#[command(
    name = "lazytop",
    version,
    about = "A lazy-style terminal system monitor for macOS (Apple Silicon) and Linux"
)]
struct Cli {
    /// Refresh interval in milliseconds (250–10000)
    #[arg(short, long)]
    interval: Option<u64>,
    /// Use synthetic data instead of reading the system
    #[arg(long)]
    demo: bool,
    /// Config file (default: ~/.config/lazytop/config.toml)
    #[arg(long)]
    config: Option<PathBuf>,
    /// Write diagnostics (missing sensors, IOReport channels) to this file
    #[arg(long)]
    log: Option<PathBuf>,
    /// Theme: dark, light, mono, catppuccin (mocha), catppuccin-macchiato, catppuccin-frappe, catppuccin-latte
    #[arg(long)]
    theme: Option<String>,
    /// Render one frame as text (WIDTHxHEIGHT, e.g. 160x48) and exit
    #[arg(long, hide = true, value_name = "WxH")]
    dump: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut cfg = match Config::load(cli.config.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("lazytop: {e:#}");
            return ExitCode::from(2);
        }
    };
    if let Some(i) = cli.interval {
        cfg.interval_ms = config::clamp_interval(i);
    }
    if let Some(t) = cli.theme {
        cfg.theme = t;
    }
    let theme = match Theme::by_name(&cfg.theme).map(|t| t.with_overrides(&cfg.colors)) {
        Some(Ok(t)) => t,
        Some(Err(e)) => {
            eprintln!("lazytop: {e:#}");
            return ExitCode::from(2);
        }
        None => {
            eprintln!(
                "lazytop: unknown theme {:?} ({})",
                cfg.theme,
                config::THEME_NAMES.join(", ")
            );
            return ExitCode::from(2);
        }
    };
    if let Some(p) = &cli.log
        && let Err(e) = lazytop::log::init(p)
    {
        eprintln!("lazytop: cannot open log {}: {e}", p.display());
        return ExitCode::from(2);
    }

    let mut collector: Box<dyn Collector> = if cli.demo {
        Box::new(DemoCollector::new(1))
    } else {
        collector::system_collector()
    };

    if let Some(dim) = cli.dump {
        let Some((w, h)) = dim
            .split_once('x')
            .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        else {
            eprintln!("lazytop: --dump expects WIDTHxHEIGHT");
            return ExitCode::from(2);
        };
        let mut app = App::new(cfg, theme, None, Box::new(process::send_signal));
        app.on_snapshot(collector.sample());
        std::thread::sleep(Duration::from_millis(500));
        app.on_snapshot(collector.sample());
        println!("{}", ui::dump(&mut app, w, h));
        return ExitCode::SUCCESS;
    }

    let interval = Duration::from_millis(cfg.interval_ms);
    let (rx, ctl) = collector::spawn(collector, interval);
    let app = App::new(cfg, theme, Some(ctl), Box::new(process::send_signal));
    match ui::run(app, rx) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lazytop: {e:#}");
            ExitCode::FAILURE
        }
    }
}

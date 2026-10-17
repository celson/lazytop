use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;

pub const MIN_INTERVAL_MS: u64 = 250;
pub const MAX_INTERVAL_MS: u64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortKey {
    #[default]
    Cpu,
    Mem,
    Pid,
    Name,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PanelsCfg {
    pub hide_empty: bool,
    pub hidden: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ProcCfg {
    pub sort: SortKey,
    pub reverse: bool,
    pub tree: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub interval_ms: u64,
    pub theme: String,
    pub panels: PanelsCfg,
    pub processes: ProcCfg,
    pub colors: BTreeMap<String, String>,
}

pub const THEME_NAMES: [&str; 8] = [
    "dark",
    "light",
    "mono",
    "catppuccin",
    "catppuccin-mocha",
    "catppuccin-macchiato",
    "catppuccin-frappe",
    "catppuccin-latte",
];

pub const PANEL_NAMES: [&str; 8] = [
    "ai_workload",
    "ai_runtime",
    "cpu",
    "gpu",
    "memory",
    "netdisk",
    "sensors",
    "processes",
];

impl SortKey {
    pub fn next(self) -> SortKey {
        match self {
            SortKey::Cpu => SortKey::Mem,
            SortKey::Mem => SortKey::Pid,
            SortKey::Pid => SortKey::Name,
            SortKey::Name => SortKey::Cpu,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Cpu => "CPU%",
            SortKey::Mem => "MEM",
            SortKey::Pid => "PID",
            SortKey::Name => "NAME",
        }
    }
}

impl Default for PanelsCfg {
    fn default() -> Self {
        Self {
            hide_empty: true,
            hidden: Vec::new(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interval_ms: 1000,
            theme: "dark".into(),
            panels: PanelsCfg::default(),
            processes: ProcCfg::default(),
            colors: BTreeMap::new(),
        }
    }
}

pub fn clamp_interval(ms: u64) -> u64 {
    ms.clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS)
}

impl Config {
    pub fn default_path() -> Option<PathBuf> {
        let base = if cfg!(target_os = "macos") {
            dirs::home_dir().map(|h| h.join(".config"))
        } else {
            dirs::config_dir()
        };
        base.map(|b| b.join("lazytop").join("config.toml"))
    }

    pub fn parse(s: &str) -> anyhow::Result<Config> {
        let c: Config = toml::from_str(s).context("invalid config")?;
        c.validate()?;
        Ok(c)
    }

    fn validate(&self) -> anyhow::Result<()> {
        if !(MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&self.interval_ms) {
            bail!("interval_ms must be between {MIN_INTERVAL_MS} and {MAX_INTERVAL_MS}");
        }
        if !THEME_NAMES.contains(&self.theme.as_str()) {
            bail!(
                "theme must be one of {} (got {:?})",
                THEME_NAMES.join(", "),
                self.theme
            );
        }
        for h in &self.panels.hidden {
            if !PANEL_NAMES.contains(&h.as_str()) {
                bail!(
                    "unknown panel {h:?} in panels.hidden; valid: {}",
                    PANEL_NAMES.join(", ")
                );
            }
        }
        Ok(())
    }

    /// Explicit path: must exist.
    pub fn load(path: Option<&Path>) -> anyhow::Result<Config> {
        match path {
            Some(p) => {
                let s = std::fs::read_to_string(p)
                    .with_context(|| format!("reading {}", p.display()))?;
                Self::parse(&s).with_context(|| format!("in {}", p.display()))
            }
            None => match Self::default_path() {
                Some(p) => Self::load_or_default(&p),
                None => Ok(Config::default()),
            },
        }
    }

    /// Default path: missing file → defaults.
    pub fn load_or_default(p: &Path) -> anyhow::Result<Config> {
        if !p.exists() {
            return Ok(Config::default());
        }
        Self::load(Some(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let c = Config::parse("").unwrap();
        assert_eq!(c.interval_ms, 1000);
        assert_eq!(c.theme, "dark");
        assert!(c.panels.hide_empty);
        assert_eq!(c.processes.sort, SortKey::Cpu);
    }

    #[test]
    fn full_file() {
        let c = Config::parse(
            r##"
interval_ms = 500
theme = "light"
[panels]
hide_empty = false
hidden = ["ai_runtime"]
[processes]
sort = "mem"
reverse = true
tree = true
[colors]
accent = "#5b8def"
"##,
        )
        .unwrap();
        assert_eq!(c.interval_ms, 500);
        assert_eq!(c.panels.hidden, vec!["ai_runtime"]);
        assert_eq!(c.processes.sort, SortKey::Mem);
        assert_eq!(c.colors["accent"], "#5b8def");
    }

    #[test]
    fn rejects_unknown_key_and_bad_values() {
        assert!(Config::parse("foo = 1").is_err());
        assert!(Config::parse("theme = \"neon\"").is_err());
        assert!(Config::parse("theme = \"catppuccin-frappe\"").is_ok());
        assert!(Config::parse("interval_ms = 10").is_err());
        assert!(Config::parse("[panels]\nhidden=[\"nope\"]").is_err());
    }

    #[test]
    fn explicit_missing_path_errors_default_missing_ok() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("none.toml");
        assert!(Config::load(Some(&p)).is_err());
        assert_eq!(Config::load_or_default(&p).unwrap(), Config::default());
    }

    #[test]
    fn clamp() {
        assert_eq!(clamp_interval(1), 250);
        assert_eq!(clamp_interval(99_999), 10_000);
        assert_eq!(clamp_interval(700), 700);
    }

    #[test]
    fn sort_cycle() {
        assert_eq!(SortKey::Cpu.next(), SortKey::Mem);
        assert_eq!(SortKey::Name.next(), SortKey::Cpu);
    }
}

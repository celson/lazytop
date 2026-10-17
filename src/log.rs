//! Minimal file logger. Without `init` everything is dropped (never printed to the TUI).
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

static FILE: OnceLock<Mutex<File>> = OnceLock::new();
static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub fn init(path: &Path) -> std::io::Result<()> {
    let f = File::create(path)?;
    let _ = FILE.set(Mutex::new(f));
    Ok(())
}

fn seen_set() -> &'static Mutex<HashSet<String>> {
    SEEN.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Logs `msg` the first time `source` reports a problem; later calls are ignored.
pub fn warn_once(source: &str, msg: &str) {
    let first = seen_set()
        .lock()
        .map(|mut s| s.insert(source.to_string()))
        .unwrap_or(false);
    if first {
        info(&format!("WARN [{source}] {msg}"));
    }
}

pub fn seen(source: &str) -> bool {
    seen_set()
        .lock()
        .map(|s| s.contains(source))
        .unwrap_or(false)
}

pub fn info(msg: &str) {
    if let Some(f) = FILE.get()
        && let Ok(mut f) = f.lock()
    {
        let _ = writeln!(f, "{msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warn_once_marks_seen() {
        assert!(!seen("test-src-x"));
        warn_once("test-src-x", "boom");
        warn_once("test-src-x", "boom again");
        assert!(seen("test-src-x"));
    }
}

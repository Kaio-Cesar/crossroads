//! Recently routed links, so rules can be written from real examples
//! (including the detected source app). Kept in `%LOCALAPPDATA%`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::config::Target;

pub const MAX_ENTRIES: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Unix seconds.
    pub time: u64,
    pub url: String,
    pub source: Option<String>,
    /// Where it opened; `None` if the picker was dismissed.
    pub target: Option<Target>,
    /// Id of the matching rule, if any.
    pub rule: Option<String>,
}

impl Entry {
    pub fn now(url: &str, source: Option<String>, target: Option<Target>, rule: Option<String>) -> Self {
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self { time, url: url.into(), source, target, rule }
    }
}

/// `%LOCALAPPDATA%\Crossroads\history.json`, or `$CROSSROADS_HISTORY` if set.
pub fn default_path() -> PathBuf {
    match std::env::var_os("CROSSROADS_HISTORY") {
        Some(path) if !path.is_empty() => path.into(),
        _ => super::local_dir().join("history.json"),
    }
}

/// Newest first. A missing or corrupt file is an empty history.
pub fn load(path: &Path) -> Vec<Entry> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn push(path: &Path, entry: Entry) -> anyhow::Result<()> {
    let mut entries = load(path);
    entries.insert(0, entry);
    entries.truncate(MAX_ENTRIES);
    super::write_atomic(path, serde_json::to_string_pretty(&entries)?.as_bytes())
}

pub fn clear(path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tempdir::Dir;

    #[test]
    fn keeps_newest_first_and_caps() {
        let dir = Dir::new();
        let path = dir.path().join("h.json");
        assert!(load(&path).is_empty());
        for i in 0..MAX_ENTRIES + 5 {
            push(&path, Entry::now(&format!("https://e.com/{i}"), Some("slack.exe".into()), None, None)).unwrap();
        }
        let entries = load(&path);
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0].url, format!("https://e.com/{}", MAX_ENTRIES + 4));
        clear(&path).unwrap();
        clear(&path).unwrap();
        assert!(load(&path).is_empty());
    }
}


//! User settings: routing rules, fallback behaviour and custom browsers.
//! Stored as TOML in `%APPDATA%\Crossroads\config.toml` so it is easy to
//! hand-edit and sync.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const CONFIG_FILE: &str = "config.toml";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// What to do with links no rule matches.
    pub fallback: Fallback,
    /// Where unmatched links go when `fallback` is `Default`.
    pub default_target: Option<Target>,
    /// Evaluated top to bottom; the first enabled match wins.
    pub rules: Vec<Rule>,
    /// Browser ids left out of the picker and target lists.
    pub hidden_browsers: Vec<String>,
    /// Browsers that aren't registered with Windows.
    pub custom_browsers: Vec<CustomBrowser>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fallback {
    /// Open in `default_target` (or show the picker if none is set).
    #[default]
    Default,
    /// Always ask.
    Picker,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub matchers: Vec<Matcher>,
    pub target: Target,
}

fn yes() -> bool {
    true
}

/// One condition. A rule matches when any of its URL conditions match and,
/// if it has source-app conditions, any of those match too.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Matcher {
    /// Host equals the domain or is a subdomain of it (`github.com` matches `gist.github.com`).
    Domain(String),
    /// Glob over the whole URL (`*://*.atlassian.net/*`).
    Wildcard(String),
    /// Regular expression over the whole URL.
    Regex(String),
    /// File name of the app that opened the link (`slack.exe`), case-insensitive.
    SourceApp(String),
}

impl Matcher {
    pub fn kind(&self) -> MatcherKind {
        match self {
            Self::Domain(_) => MatcherKind::Domain,
            Self::Wildcard(_) => MatcherKind::Wildcard,
            Self::Regex(_) => MatcherKind::Regex,
            Self::SourceApp(_) => MatcherKind::SourceApp,
        }
    }

    pub fn value(&self) -> &str {
        match self {
            Self::Domain(v) | Self::Wildcard(v) | Self::Regex(v) | Self::SourceApp(v) => v,
        }
    }

    pub fn is_source(&self) -> bool {
        matches!(self, Self::SourceApp(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatcherKind {
    Domain,
    Wildcard,
    Regex,
    SourceApp,
}

impl MatcherKind {
    pub const ALL: [Self; 4] = [Self::Domain, Self::Wildcard, Self::Regex, Self::SourceApp];

    pub fn label(self) -> &'static str {
        match self {
            Self::Domain => "Domain",
            Self::Wildcard => "Wildcard",
            Self::Regex => "Regex",
            Self::SourceApp => "Opened from app",
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Self::Domain => "github.com",
            Self::Wildcard => "*://*.atlassian.net/*",
            Self::Regex => r"^https://(www\.)?youtube\.com/",
            Self::SourceApp => "slack.exe",
        }
    }

    pub fn make(self, value: String) -> Matcher {
        match self {
            Self::Domain => Matcher::Domain(value),
            Self::Wildcard => Matcher::Wildcard(value),
            Self::Regex => Matcher::Regex(value),
            Self::SourceApp => Matcher::SourceApp(value),
        }
    }
}

/// Where a link opens.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Target {
    /// Browser id (`StartMenuInternet` key name, or `custom:<name>`).
    pub browser: String,
    /// Chromium profile directory or Firefox profile name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub private: bool,
}

/// A browser added by hand: an executable plus an argument template where
/// `{url}` is replaced by the link (appended if the template lacks it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomBrowser {
    pub name: String,
    pub exe: PathBuf,
    #[serde(default)]
    pub args: String,
}

impl CustomBrowser {
    pub fn id(&self) -> String {
        format!("custom:{}", self.name)
    }
}

impl Config {
    /// `%APPDATA%\Crossroads\config.toml`, or `$CROSSROADS_CONFIG` if set
    /// (handy for trying rules without touching the real config).
    pub fn default_path() -> PathBuf {
        match std::env::var_os("CROSSROADS_CONFIG") {
            Some(path) if !path.is_empty() => path.into(),
            _ => super::config_dir().join(CONFIG_FILE),
        }
    }

    /// Loads the config; a missing file is an empty config.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).with_context(|| format!("parsing {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self)?;
        super::write_atomic(path, text.as_bytes())
    }

    /// A rule id not used yet.
    pub fn new_rule_id(&self) -> String {
        (1..)
            .map(|n| format!("rule-{n}"))
            .find(|id| self.rules.iter().all(|r| &r.id != id))
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tempdir::Dir;

    fn sample() -> Config {
        Config {
            fallback: Fallback::Picker,
            default_target: Some(Target { browser: "FIREFOX.EXE".into(), profile: None, private: false }),
            rules: vec![Rule {
                id: "rule-1".into(),
                name: "Work".into(),
                enabled: true,
                matchers: vec![
                    Matcher::Domain("atlassian.net".into()),
                    Matcher::Regex(r"^https://x\.com/".into()),
                    Matcher::SourceApp("slack.exe".into()),
                ],
                target: Target { browser: "Google Chrome".into(), profile: Some("Profile 1".into()), private: true },
            }],
            hidden_browsers: vec!["IEXPLORE.EXE".into()],
            custom_browsers: vec![CustomBrowser {
                name: "Portable".into(),
                exe: r"C:\Tools\browser.exe".into(),
                args: "--new-window {url}".into(),
            }],
        }
    }

    #[test]
    fn round_trips_through_toml() {
        let dir = Dir::new();
        let path = dir.path().join("sub").join(CONFIG_FILE);
        assert_eq!(Config::load(&path).unwrap(), Config::default());
        let config = sample();
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
    }

    #[test]
    fn hand_written_config_parses() {
        let text = r#"
            fallback = "default"
            default_target = { browser = "FIREFOX.EXE" }

            [[rules]]
            id = "r"
            name = "GitHub"
            matchers = [{ type = "domain", value = "github.com" }]
            target = { browser = "Microsoft Edge", private = true }
        "#;
        let c: Config = toml::from_str(text).unwrap();
        assert!(c.rules[0].enabled);
        assert_eq!(c.rules[0].matchers, vec![Matcher::Domain("github.com".into())]);
        assert!(c.rules[0].target.private);
    }

    #[test]
    fn new_rule_ids_are_unique() {
        let mut c = sample();
        assert_eq!(c.new_rule_id(), "rule-2");
        c.rules.clear();
        assert_eq!(c.new_rule_id(), "rule-1");
    }
}

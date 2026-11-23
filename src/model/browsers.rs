//! Finds installed browsers (from the Windows `StartMenuInternet` registry
//! keys) and their profiles (Chromium `Local State`, Firefox `profiles.ini`).

use std::path::{Path, PathBuf};

use super::config::{Config, Target};

/// Registry key name Crossroads registers itself under (skipped in discovery).
pub const SELF_KEY: &str = "Crossroads";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Chromium,
    Firefox,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// What the browser is launched with: Chromium profile directory or Firefox profile name.
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Browser {
    /// Stable id: `StartMenuInternet` key name or `custom:<name>`.
    pub id: String,
    pub name: String,
    pub exe: PathBuf,
    pub family: Family,
    pub profiles: Vec<Profile>,
    /// Argument template for custom browsers (`{url}` placeholder).
    pub custom_args: Option<String>,
}

/// One selectable destination: a browser, optionally a profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetOption {
    pub target: Target,
    pub label: String,
    pub exe: PathBuf,
}

impl Browser {
    /// File stem of the executable, lowercased (`chrome`, `msedge`, …).
    pub fn stem(&self) -> String {
        exe_stem(&self.exe)
    }

    pub fn supports_private(&self) -> bool {
        self.family != Family::Other
    }

    pub fn profile_name(&self, id: &str) -> Option<&str> {
        self.profiles.iter().find(|p| p.id == id).map(|p| p.name.as_str())
    }
}

pub fn exe_stem(exe: &Path) -> String {
    exe.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

pub fn family_of(stem: &str) -> Family {
    match stem {
        "chrome" | "msedge" | "brave" | "vivaldi" | "chromium" | "thorium" | "cent" | "yandex" => Family::Chromium,
        "firefox" | "librewolf" | "waterfox" | "zen" | "floorp" | "mullvadbrowser" => Family::Firefox,
        _ => Family::Other,
    }
}

/// Extracts the executable from a `shell\open\command` value such as
/// `"C:\Program Files\Google\Chrome\Application\chrome.exe" --single-argument %1`.
pub fn exe_from_command(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    let exe = if let Some(rest) = command.strip_prefix('"') {
        rest.split('"').next()?
    } else {
        let lower = command.to_ascii_lowercase();
        let end = lower.find(".exe").map(|i| i + 4).unwrap_or(command.len());
        &command[..end]
    };
    let exe = exe.trim();
    (!exe.is_empty()).then(|| PathBuf::from(exe))
}

/// Chromium user-data directory for a browser executable.
fn chromium_user_data(exe: &Path) -> Option<PathBuf> {
    // Per-user installs (Thorium, Helium, Vivaldi, …) keep `User Data` next
    // to the `Application` folder holding the executable.
    let sibling = exe.parent()?.parent()?.join("User Data");
    if sibling.join("Local State").exists() {
        return Some(sibling);
    }
    let local = dirs::data_local_dir()?;
    let path = exe.to_string_lossy().to_ascii_lowercase();
    // Forks reuse `chrome.exe`/`msedge.exe`; only trust the vendor folders
    // when the executable actually lives in them.
    let vendor = |marker: &str| path.contains(marker);
    let channel = |base: &str, variants: &[(&str, &str)]| {
        variants
            .iter()
            .find(|(marker, _)| path.contains(marker))
            .map(|(_, dir)| dir.to_string())
            .unwrap_or_else(|| base.to_string())
    };
    let rel = match exe_stem(exe).as_str() {
        "chrome" if vendor(r"google\chrome") => channel(
            r"Google\Chrome",
            &[("chrome sxs", r"Google\Chrome SxS"), ("chrome beta", r"Google\Chrome Beta"), ("chrome dev", r"Google\Chrome Dev")],
        ),
        "msedge" if vendor(r"microsoft\edge") => channel(
            r"Microsoft\Edge",
            &[("edge sxs", r"Microsoft\Edge SxS"), ("edge beta", r"Microsoft\Edge Beta"), ("edge dev", r"Microsoft\Edge Dev")],
        ),
        "brave" => channel(
            r"BraveSoftware\Brave-Browser",
            &[("brave-browser-beta", r"BraveSoftware\Brave-Browser-Beta"), ("brave-browser-nightly", r"BraveSoftware\Brave-Browser-Nightly")],
        ),
        "vivaldi" => "Vivaldi".into(),
        "chromium" => "Chromium".into(),
        "thorium" => "Thorium".into(),
        "cent" => "CentBrowser".into(),
        "yandex" => r"Yandex\YandexBrowser".into(),
        _ => return None,
    };
    Some(local.join(rel).join("User Data"))
}

/// Firefox-family `profiles.ini` for a browser executable.
fn firefox_profiles_ini(exe: &Path) -> Option<PathBuf> {
    let roaming = dirs::config_dir()?;
    let rel = match exe_stem(exe).as_str() {
        "firefox" => r"Mozilla\Firefox",
        "librewolf" => "librewolf",
        "waterfox" => "Waterfox",
        "zen" => "zen",
        "floorp" => "Floorp",
        "mullvadbrowser" => r"Mullvad\MullvadBrowser",
        _ => return None,
    };
    Some(roaming.join(rel).join("profiles.ini"))
}

/// Parses `profile.info_cache` from a Chromium `Local State` file.
pub fn parse_local_state(json: &str) -> Vec<Profile> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let Some(cache) = value.pointer("/profile/info_cache").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    let mut profiles: Vec<Profile> = cache
        .iter()
        .map(|(dir, info)| Profile {
            id: dir.clone(),
            name: info.get("name").and_then(|n| n.as_str()).unwrap_or(dir).to_string(),
        })
        .collect();
    profiles.sort_by_key(|p| (p.id != "Default", natural_key(&p.id)));
    profiles
}

/// `Profile 10` sorts after `Profile 2`.
fn natural_key(s: &str) -> (String, u64) {
    let digits: String = s.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
    let n = digits.parse().unwrap_or(0);
    (s[..s.len() - digits.len()].to_string(), n)
}

/// Parses profile names from a Firefox `profiles.ini`.
pub fn parse_profiles_ini(text: &str) -> Vec<Profile> {
    let mut profiles = Vec::new();
    let mut in_profile = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_profile = line.to_ascii_lowercase().starts_with("[profile");
        } else if in_profile
            && let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("name")
        {
            let name = value.trim().to_string();
            profiles.push(Profile { id: name.clone(), name });
        }
    }
    profiles
}

fn detect_profiles(exe: &Path, family: Family) -> Vec<Profile> {
    let read = |path: Option<PathBuf>| path.and_then(|p| std::fs::read_to_string(p).ok());
    match family {
        Family::Chromium => read(chromium_user_data(exe).map(|d| d.join("Local State")))
            .map(|s| parse_local_state(&s))
            .unwrap_or_default(),
        Family::Firefox => read(firefox_profiles_ini(exe)).map(|s| parse_profiles_ini(&s)).unwrap_or_default(),
        Family::Other => Vec::new(),
    }
}

/// Browsers registered with Windows: (key name, display name, executable).
#[cfg(windows)]
fn registered() -> Vec<(String, String, PathBuf)> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};

    let roots = [
        (HKEY_CURRENT_USER, r"Software\Clients\StartMenuInternet"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Clients\StartMenuInternet"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Clients\StartMenuInternet"),
    ];
    let mut found: Vec<(String, String, PathBuf)> = Vec::new();
    for (hive, path) in roots {
        let Ok(clients) = RegKey::predef(hive).open_subkey_with_flags(path, KEY_READ) else { continue };
        for key in clients.enum_keys().filter_map(Result::ok) {
            if key.eq_ignore_ascii_case(SELF_KEY) || found.iter().any(|(k, ..)| k.eq_ignore_ascii_case(&key)) {
                continue;
            }
            let Ok(entry) = clients.open_subkey_with_flags(&key, KEY_READ) else { continue };
            let name: String = entry.get_value("").unwrap_or_else(|_| key.clone());
            let Some(exe) = entry
                .open_subkey_with_flags(r"shell\open\command", KEY_READ)
                .and_then(|c| c.get_value::<String, _>(""))
                .ok()
                .and_then(|c| exe_from_command(&c))
            else {
                continue;
            };
            if !exe.exists() || found.iter().any(|(.., e)| e == &exe) {
                continue;
            }
            found.push((key, name, exe));
        }
    }
    found
}

#[cfg(not(windows))]
fn registered() -> Vec<(String, String, PathBuf)> {
    Vec::new()
}

/// All browsers: registered ones (sorted by name) followed by custom ones.
/// Includes hidden browsers; callers filter with [`visible`].
pub fn discover(config: &Config) -> Vec<Browser> {
    let mut browsers: Vec<Browser> = registered()
        .into_iter()
        .map(|(id, name, exe)| {
            let family = family_of(&exe_stem(&exe));
            Browser { profiles: detect_profiles(&exe, family), id, name, exe, family, custom_args: None }
        })
        .collect();
    browsers.sort_by_key(|b| b.name.to_lowercase());
    browsers.extend(config.custom_browsers.iter().map(|c| Browser {
        id: c.id(),
        name: c.name.clone(),
        exe: c.exe.clone(),
        family: Family::Other,
        profiles: Vec::new(),
        custom_args: Some(c.args.clone()),
    }));
    browsers
}

pub fn find<'a>(browsers: &'a [Browser], id: &str) -> Option<&'a Browser> {
    browsers.iter().find(|b| b.id == id)
}

/// Whether `target` can be opened: its browser exists and so does its profile.
pub fn is_available(browsers: &[Browser], target: &Target) -> bool {
    find(browsers, &target.browser).is_some_and(|b| match &target.profile {
        None => true,
        Some(p) => b.profiles.iter().any(|x| &x.id == p),
    })
}

/// Human-readable name of a target, e.g. `Google Chrome · Work (private)`.
pub fn target_label(browsers: &[Browser], target: &Target) -> String {
    let Some(b) = find(browsers, &target.browser) else {
        return format!("{} (not installed)", target.browser);
    };
    let mut label = b.name.clone();
    if let Some(p) = &target.profile {
        label.push_str(" · ");
        label.push_str(b.profile_name(p).unwrap_or(p));
    }
    if target.private {
        label.push_str(" (private)");
    }
    label
}

pub fn visible<'a>(browsers: &'a [Browser], config: &'a Config) -> impl Iterator<Item = &'a Browser> {
    browsers.iter().filter(|b| !config.hidden_browsers.contains(&b.id))
}

/// Every selectable destination among visible browsers: each browser, plus
/// each of its profiles when it has more than one.
pub fn target_options(browsers: &[Browser], config: &Config) -> Vec<TargetOption> {
    let mut out = Vec::new();
    for b in visible(browsers, config) {
        let base = Target { browser: b.id.clone(), profile: None, private: false };
        if b.profiles.len() > 1 {
            for p in &b.profiles {
                let target = Target { profile: Some(p.id.clone()), ..base.clone() };
                out.push(TargetOption { label: target_label(browsers, &target), target, exe: b.exe.clone() });
            }
        } else {
            out.push(TargetOption { label: b.name.clone(), target: base, exe: b.exe.clone() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        assert_eq!(
            exe_from_command(r#""C:\Program Files\Google\Chrome\Application\chrome.exe" --single-argument %1"#),
            Some(PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"))
        );
        assert_eq!(
            exe_from_command(r"C:\Program Files\Mozilla Firefox\firefox.exe -osint -url %1"),
            Some(PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe"))
        );
        assert_eq!(exe_from_command("  "), None);
    }

    #[test]
    fn classifies_families() {
        assert_eq!(family_of(&exe_stem(Path::new(r"C:\x\MSEDGE.EXE"))), Family::Chromium);
        assert_eq!(family_of("librewolf"), Family::Firefox);
        assert_eq!(family_of("opera"), Family::Other);
    }

    #[test]
    fn parses_chromium_local_state() {
        let json = r#"{"profile":{"info_cache":{
            "Profile 10":{"name":"Ten"},
            "Profile 2":{"name":"Work"},
            "Default":{"name":"Personal"},
            "Guest":{}
        }}}"#;
        let ids: Vec<_> = parse_local_state(json).into_iter().map(|p| (p.id, p.name)).collect();
        assert_eq!(
            ids,
            vec![
                ("Default".into(), "Personal".into()),
                ("Guest".into(), "Guest".into()),
                ("Profile 2".into(), "Work".into()),
                ("Profile 10".into(), "Ten".into()),
            ]
        );
        assert!(parse_local_state("not json").is_empty());
    }

    #[test]
    fn parses_firefox_profiles_ini() {
        let ini = "[Install308046B0AF4A39CB]\nDefault=Profiles/abc.default-release\n\n\
                   [Profile1]\nName=work\nIsRelative=1\nPath=Profiles/x.work\n\n\
                   [Profile0]\nName=default-release\nPath=Profiles/abc.default-release\n\n\
                   [General]\nStartWithLastProfile=1\nName=nope\n";
        let names: Vec<_> = parse_profiles_ini(ini).into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["work", "default-release"]);
    }

    fn browser(id: &str, profiles: &[&str]) -> Browser {
        Browser {
            id: id.into(),
            name: id.to_uppercase(),
            exe: format!(r"C:\{id}.exe").into(),
            family: Family::Chromium,
            profiles: profiles.iter().map(|p| Profile { id: p.to_string(), name: format!("{p}!") }).collect(),
            custom_args: None,
        }
    }

    #[test]
    fn target_options_and_labels() {
        let browsers = vec![browser("a", &["Default"]), browser("b", &["Default", "Profile 1"]), browser("c", &[])];
        let config = Config { hidden_browsers: vec!["c".into()], ..Default::default() };
        let labels: Vec<_> = target_options(&browsers, &config).into_iter().map(|o| o.label).collect();
        assert_eq!(labels, vec!["A", "B · Default!", "B · Profile 1!"]);

        let t = Target { browser: "b".into(), profile: Some("Profile 1".into()), private: true };
        assert_eq!(target_label(&browsers, &t), "B · Profile 1! (private)");
        assert!(is_available(&browsers, &t));
        assert!(!is_available(&browsers, &Target { profile: Some("Profile 9".into()), ..t.clone() }));
        assert!(!is_available(&browsers, &Target { browser: "zzz".into(), ..Default::default() }));
    }
}

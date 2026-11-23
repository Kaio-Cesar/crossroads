//! Registers Crossroads as a browser for the current user (HKCU only, no
//! admin rights). Windows doesn't let apps make themselves the default, so
//! the user confirms it in Settings › Default apps.

use std::path::{Path, PathBuf};

use super::browsers::SELF_KEY;

pub const URL_PROGID: &str = "CrossroadsURL";
pub const HTML_PROGID: &str = "CrossroadsHTML";
const CLIENT_KEY: &str = r"Software\Clients\StartMenuInternet\Crossroads";
const SETTINGS_URI: &str = "ms-settings:defaultapps?registeredAppUser=Crossroads";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    NotRegistered,
    /// Registered, but pointing at another executable (it was moved/rebuilt elsewhere).
    Outdated,
    Registered,
}

/// The `shell\open\command` value for this executable.
pub fn open_command(exe: &Path) -> String {
    format!("\"{}\" --url \"%1\"", exe.display())
}

pub fn current_exe() -> PathBuf {
    std::env::current_exe().unwrap_or_default()
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::path::Path;

    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

    use super::*;

    fn hkcu() -> RegKey {
        RegKey::predef(HKEY_CURRENT_USER)
    }

    fn set(path: &str, name: &str, value: &str) -> io::Result<()> {
        let (key, _) = hkcu().create_subkey(path)?;
        key.set_value(name, &value.to_string())
    }

    fn get(path: &str, name: &str) -> Option<String> {
        hkcu().open_subkey_with_flags(path, KEY_READ).ok()?.get_value(name).ok()
    }

    fn register_progid(progid: &str, description: &str, icon: &str, command: &str, protocol: bool) -> io::Result<()> {
        let base = format!(r"Software\Classes\{progid}");
        set(&base, "", description)?;
        if protocol {
            set(&base, "URL Protocol", "")?;
        }
        set(&format!(r"{base}\DefaultIcon"), "", icon)?;
        set(&format!(r"{base}\shell\open\command"), "", command)
    }

    pub fn register(exe: &Path) -> io::Result<()> {
        let icon = format!("\"{}\",0", exe.display());
        let command = open_command(exe);
        register_progid(URL_PROGID, "Crossroads URL", &icon, &command, true)?;
        register_progid(HTML_PROGID, "Crossroads HTML Document", &icon, &command, false)?;

        set(CLIENT_KEY, "", "Crossroads")?;
        set(&format!(r"{CLIENT_KEY}\DefaultIcon"), "", &icon)?;
        set(&format!(r"{CLIENT_KEY}\shell\open\command"), "", &format!("\"{}\"", exe.display()))?;
        let caps = format!(r"{CLIENT_KEY}\Capabilities");
        set(&caps, "ApplicationName", "Crossroads")?;
        set(&caps, "ApplicationDescription", "Opens each link in the right browser, based on your rules.")?;
        set(&caps, "ApplicationIcon", &icon)?;
        set(&format!(r"{caps}\StartMenu"), "StartMenuInternet", SELF_KEY)?;
        for scheme in ["http", "https"] {
            set(&format!(r"{caps}\URLAssociations"), scheme, URL_PROGID)?;
        }
        for ext in [".htm", ".html", ".shtml", ".xhtml"] {
            set(&format!(r"{caps}\FileAssociations"), ext, HTML_PROGID)?;
        }
        set(r"Software\RegisteredApplications", "Crossroads", &caps)?;
        notify();
        Ok(())
    }

    pub fn unregister() -> io::Result<()> {
        let root = hkcu();
        for path in [CLIENT_KEY.to_string(), format!(r"Software\Classes\{URL_PROGID}"), format!(r"Software\Classes\{HTML_PROGID}")] {
            match root.delete_subkey_all(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        if let Ok(apps) = root.open_subkey_with_flags(r"Software\RegisteredApplications", winreg::enums::KEY_ALL_ACCESS) {
            let _ = apps.delete_value("Crossroads");
        }
        notify();
        Ok(())
    }

    pub fn status(exe: &Path) -> Status {
        match get(&format!(r"Software\Classes\{URL_PROGID}\shell\open\command"), "") {
            None => Status::NotRegistered,
            Some(c) if c.eq_ignore_ascii_case(&open_command(exe)) => Status::Registered,
            Some(_) => Status::Outdated,
        }
    }

    pub fn is_default() -> bool {
        let base = r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https";
        [format!(r"{base}\UserChoiceLatest"), format!(r"{base}\UserChoice")]
            .iter()
            .find_map(|k| get(k, "ProgId"))
            .is_some_and(|p| p.eq_ignore_ascii_case(URL_PROGID))
    }

    fn notify() {
        use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
    }
}

#[cfg(windows)]
pub use imp::{is_default, register, status, unregister};

#[cfg(not(windows))]
pub fn register(_: &Path) -> std::io::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn unregister() -> std::io::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn status(_: &Path) -> Status {
    Status::NotRegistered
}
#[cfg(not(windows))]
pub fn is_default() -> bool {
    false
}

/// Opens Settings › Default apps on Crossroads' page.
pub fn open_default_apps_settings() -> std::io::Result<()> {
    std::process::Command::new("explorer").arg(SETTINGS_URI).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    #[test]
    fn command_quotes_exe_and_url() {
        let cmd = super::open_command(std::path::Path::new(r"C:\Program Files\Crossroads\crossroads.exe"));
        assert_eq!(cmd, r#""C:\Program Files\Crossroads\crossroads.exe" --url "%1""#);
    }
}

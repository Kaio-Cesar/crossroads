//! Best-effort detection of the app a link was opened from.
//!
//! Most desktop apps open links with `ShellExecute`, which makes them our
//! parent process. Packaged apps and some shell paths go through a broker
//! (`svchost`, `RuntimeBroker`, …); then the app owning the foreground window
//! — the one the user just clicked in — is the better guess.

/// Processes that launch handlers on behalf of another app.
const BROKERS: &[&str] = &[
    "explorer.exe",
    "svchost.exe",
    "runtimebroker.exe",
    "sihost.exe",
    "openwith.exe",
    "applicationframehost.exe",
    "crossroads.exe",
];

pub fn is_broker(exe: &str) -> bool {
    BROKERS.iter().any(|b| b.eq_ignore_ascii_case(exe))
}

/// Lowercased file name of the app that most likely opened the link.
#[cfg(windows)]
pub fn detect() -> Option<String> {
    use std::collections::HashMap;

    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    // pid -> (parent pid, exe name)
    let mut table: HashMap<u32, (u32, String)> = HashMap::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_ascii_lowercase();
                table.insert(entry.th32ProcessID, (entry.th32ParentProcessID, name));
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }

    let parent = table.get(&std::process::id()).and_then(|(ppid, _)| table.get(ppid)).map(|(_, n)| n.clone());
    if let Some(parent) = &parent
        && !is_broker(parent)
    {
        return Some(parent.clone());
    }

    let foreground = unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        table.get(&pid).map(|(_, n)| n.clone())
    };
    foreground.filter(|f| !is_broker(f)).or(parent)
}

#[cfg(not(windows))]
pub fn detect() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn brokers_are_case_insensitive() {
        assert!(super::is_broker("RuntimeBroker.exe"));
        assert!(!super::is_broker("slack.exe"));
    }
}

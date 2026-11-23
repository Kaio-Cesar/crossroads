//! The `--url` fast path: decide and launch without starting the UI.

use crate::model::config::Config;
use crate::model::rules::{self, Decision, LinkContext};
use crate::model::{browsers, history, launch, source};

/// Parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// No arguments: the settings window.
    Settings,
    /// A link to route (`--url <URL>`).
    Route(String),
    /// Always show the picker (`--pick <URL>`).
    Pick(String),
    /// Print detected browsers and registration state (`--diagnose`).
    Diagnose,
}

impl Mode {
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Self {
        let mut args = args.into_iter().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--diagnose" {
                return Self::Diagnose;
            }
            for (flag, make) in [("--url", Self::Route as fn(String) -> Self), ("--pick", Self::Pick)] {
                if arg == flag
                    && let Some(url) = args.next()
                {
                    return make(url);
                }
                if let Some(url) = arg.strip_prefix(flag).and_then(|r| r.strip_prefix('=')) {
                    return make(url.to_string());
                }
            }
            // A bare URL or file path (e.g. dropped onto the exe).
            if !arg.starts_with("--") {
                return Self::Route(arg);
            }
        }
        Self::Settings
    }
}

/// Prints what Crossroads sees: browsers, profiles, registration.
pub fn diagnose() {
    use crate::model::registration;
    let config = Config::load(&Config::default_path()).unwrap_or_default();
    println!("config: {}", Config::default_path().display());
    for b in browsers::discover(&config) {
        println!("{} [{}] {:?} {}", b.name, b.id, b.family, b.exe.display());
        for p in &b.profiles {
            println!("    profile {:?} = {}", p.id, p.name);
        }
    }
    let exe = registration::current_exe();
    println!("registration: {:?}, default: {}", registration::status(&exe), registration::is_default());
    println!("source app: {:?}", source::detect());
}

/// True while Shift is physically held.
fn shift_held() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_SHIFT};
        return GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000 != 0;
    }
    #[allow(unreachable_code)]
    false
}

/// Everything the picker needs when routing can't decide on its own.
pub struct PickRequest {
    pub ctx: LinkContext,
    pub config: Config,
    pub browsers: Vec<browsers::Browser>,
}

/// Routes `url`. Returns `Some` when the picker must be shown.
pub fn route(url: String, force_picker: bool) -> Option<PickRequest> {
    let config = Config::load(&Config::default_path()).unwrap_or_else(|e| {
        eprintln!("config: {e:#}");
        Config::default()
    });
    let browsers = browsers::discover(&config);
    let ctx = LinkContext { url, source_exe: source::detect(), force_picker: force_picker || shift_held() };
    let decision = rules::route(&config, &ctx, |t| browsers::is_available(&browsers, t));
    eprintln!("route: {} from {:?} -> {decision:?}", ctx.url, ctx.source_exe);

    if let Decision::Open { target, rule } = decision {
        match launch::open(&browsers, &target, &ctx.url) {
            Ok(()) => {
                let entry = history::Entry::now(&ctx.url, ctx.source_exe.clone(), Some(target), rule);
                if let Err(e) = history::push(&history::default_path(), entry) {
                    eprintln!("history: {e:#}");
                }
                return None;
            }
            Err(e) => eprintln!("launch failed, asking instead: {e:#}"),
        }
    }
    Some(PickRequest { ctx, config, browsers })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_command_line() {
        assert_eq!(Mode::from_args(args(&["exe"])), Mode::Settings);
        assert_eq!(Mode::from_args(args(&["exe", "--url", "https://a"])), Mode::Route("https://a".into()));
        assert_eq!(Mode::from_args(args(&["exe", "--url=https://a"])), Mode::Route("https://a".into()));
        assert_eq!(Mode::from_args(args(&["exe", "--pick", "https://a"])), Mode::Pick("https://a".into()));
        assert_eq!(Mode::from_args(args(&["exe", r"C:\x.html"])), Mode::Route(r"C:\x.html".into()));
        assert_eq!(Mode::from_args(args(&["exe", "--url"])), Mode::Settings);
    }
}

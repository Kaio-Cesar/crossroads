//! Opens a URL in a target browser/profile. The URL is always passed as its
//! own argv element, never spliced into a shell string.

use anyhow::{Context, Result, anyhow};

use super::browsers::{self, Browser, Family};
use super::config::Target;

/// Command-line arguments that open `url` in `browser` with `target`'s options.
pub fn args(browser: &Browser, target: &Target, url: &str) -> Vec<String> {
    if let Some(template) = &browser.custom_args {
        return custom_args(template, url);
    }
    let mut args = Vec::new();
    match browser.family {
        Family::Chromium => {
            if let Some(p) = &target.profile {
                args.push(format!("--profile-directory={p}"));
            }
            if target.private {
                args.push(if browser.stem() == "msedge" { "--inprivate" } else { "--incognito" }.into());
            }
            // Everything after this flag is one URL, even if it starts with `-`.
            args.push("--single-argument".into());
            args.push(url.into());
        }
        Family::Firefox => {
            if let Some(p) = &target.profile {
                args.push("-P".into());
                args.push(p.clone());
            }
            args.push(if target.private { "-private-window" } else { "-new-tab" }.into());
            args.push(url.into());
        }
        Family::Other => args.push(url.into()),
    }
    args
}

/// Splits a template on whitespace (double quotes group words) and replaces
/// `{url}`; the URL is appended when the template doesn't mention it.
fn custom_args(template: &str, url: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut has_token = false;
    for c in template.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                has_token = true;
            }
            c if c.is_whitespace() && !quoted => {
                if has_token {
                    out.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            c => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        out.push(cur);
    }
    let mut used = false;
    for a in &mut out {
        if a.contains("{url}") {
            *a = a.replace("{url}", url);
            used = true;
        }
    }
    if !used {
        out.push(url.into());
    }
    out
}

/// Opens `url` in `target`, failing if its browser isn't installed.
pub fn open(browsers: &[Browser], target: &Target, url: &str) -> Result<()> {
    let browser = browsers::find(browsers, &target.browser)
        .ok_or_else(|| anyhow!("browser “{}” is not installed", target.browser))?;
    let mut cmd = std::process::Command::new(&browser.exe);
    cmd.args(args(browser, target, url));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    if let Some(dir) = browser.exe.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn().with_context(|| format!("starting {}", browser.exe.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser(exe: &str, family: Family) -> Browser {
        Browser {
            id: "b".into(),
            name: "B".into(),
            exe: exe.into(),
            family,
            profiles: Vec::new(),
            custom_args: None,
        }
    }

    fn target(profile: Option<&str>, private: bool) -> Target {
        Target { browser: "b".into(), profile: profile.map(Into::into), private }
    }

    const URL: &str = "https://example.com/a b";

    #[test]
    fn chromium_args() {
        let chrome = browser(r"C:\chrome.exe", Family::Chromium);
        assert_eq!(args(&chrome, &target(None, false), URL), vec!["--single-argument", URL]);
        assert_eq!(
            args(&chrome, &target(Some("Profile 1"), true), URL),
            vec!["--profile-directory=Profile 1", "--incognito", "--single-argument", URL]
        );
        let edge = browser(r"C:\msedge.exe", Family::Chromium);
        assert_eq!(args(&edge, &target(None, true), URL), vec!["--inprivate", "--single-argument", URL]);
    }

    #[test]
    fn firefox_args() {
        let ff = browser(r"C:\firefox.exe", Family::Firefox);
        assert_eq!(args(&ff, &target(None, false), URL), vec!["-new-tab", URL]);
        assert_eq!(args(&ff, &target(Some("work"), true), URL), vec!["-P", "work", "-private-window", URL]);
    }

    #[test]
    fn other_and_custom_args() {
        let other = browser(r"C:\opera.exe", Family::Other);
        assert_eq!(args(&other, &target(Some("x"), true), URL), vec![URL]);

        let mut custom = browser(r"C:\x.exe", Family::Other);
        custom.custom_args = Some(r#"--new-window "--title=My Browser" --open={url}"#.into());
        assert_eq!(
            args(&custom, &target(None, false), URL),
            vec!["--new-window", "--title=My Browser", &format!("--open={URL}")]
        );
        custom.custom_args = Some("  ".into());
        assert_eq!(args(&custom, &target(None, false), URL), vec![URL]);
        custom.custom_args = Some(r#"-a """#.into());
        assert_eq!(args(&custom, &target(None, false), URL), vec!["-a", "", URL]);
    }
}

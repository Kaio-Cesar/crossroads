# Crossroads

A rule-based link router for Windows. Crossroads registers itself as your
default browser and sends every link you open to the right browser, profile
or private window, based on rules you set. It was inspired by
[Junction](https://github.com/sonnyp/Junction) on Linux, but it puts rules first.

```
github.com, *.atlassian.net   →  Chrome · Work
links opened from slack.exe   →  Edge (InPrivate)
everything else               →  Firefox     (or: ask me)
```

## Features

- **Rules, first match wins.** Rules are checked from top to bottom.
  A rule can have these conditions:
  - **Domain**: `github.com` also matches `gist.github.com`, but not `notgithub.com`.
  - **Wildcard** over the full URL, for example `*://*.atlassian.net/*`.
  - **Regex** over the full URL.
  - **Opened from app**: the app the link came from, for example `slack.exe`.

  A rule matches when any of its link conditions match. If the rule also has
  app conditions, the link must come from one of those apps too.
- **Browser profiles.** Crossroads reads Chromium profiles (Chrome, Edge,
  Brave, Vivaldi, Thorium, Helium…) from `Local State` and Firefox-family
  profiles (Firefox, Zen, LibreWolf, Waterfox, Floorp…) from `profiles.ini`.
  Each profile can be a rule target, with or without a private window.
- **Fallback.** Links that no rule matches open in your default browser, or
  Crossroads asks you which browser to use.
- **Shift to choose.** Hold Shift while you click a link to always get the picker.
- **Keyboard-first picker.** Press `1`–`9` or use the arrows and `Enter`.
  `Ctrl` opens a private window and `Esc` cancels. "Always open <domain>
  there" saves a new rule.
- **Recent links.** Shows each link with the app it came from. You can turn
  any of them into a rule in one click.
- **Rule tester.** Paste a URL to see which rule catches it and where it would open.
- **Custom browsers.** Add any executable with an argument template that
  uses `{url}`.
- **Fast.** When a rule decides, Crossroads starts the browser and exits
  without starting a UI.
- **Native look.** Windows 11 Fluent styling with a Mica backdrop. It follows
  your light/dark theme and accent color. Built with [GPUI](https://www.gpui.rs/)
  through `gpui-kit`.

## Install

Download `crossroads.exe` from the latest GitHub release. Every push also
builds it: look for the `crossroads-windows-x64` artifact on the **Build**
workflow run. Or build it yourself (you need Rust; the version is pinned in
`rust-toolchain.toml`):

```powershell
cargo build --release
```

1. Put `crossroads.exe` somewhere permanent and run it.
2. On **General**, click **Set up**. This registers Crossroads as a browser
   for your user only (HKCU, no admin needed) and opens Windows Settings.
3. In **Settings › Default apps › Crossroads**, choose **Set default**.
   Windows doesn't let apps make themselves the default, so you have to do
   this step yourself.

If you move the executable, open it once more and it re-registers itself.
**Unregister** on the General page removes every registry entry it created.

## Usage

| Command                         | What it does                                          |
| ------------------------------- | ----------------------------------------------------- |
| `crossroads`                    | Opens the settings window (single instance)           |
| `crossroads --url <URL>`        | Routes a link (this is what Windows runs)             |
| `crossroads --pick <URL>`       | Always shows the picker                               |
| `crossroads --diagnose`         | Prints detected browsers, profiles and registration   |

## Configuration

Settings are saved to `%APPDATA%\Crossroads\config.toml` when you change
them in the app. You can also edit the file by hand:

```toml
fallback = "default"            # or "picker"
default_target = { browser = "Firefox-308046B0AF4A39CB" }

[[rules]]
id = "rule-1"
name = "Work"
matchers = [
  { type = "domain", value = "atlassian.net" },
  { type = "wildcard", value = "*://*.corp.example.com/*" },
]
target = { browser = "Google Chrome", profile = "Profile 1" }

[[rules]]
id = "rule-2"
name = "From Slack"
matchers = [{ type = "source_app", value = "slack.exe" }]
target = { browser = "Microsoft Edge", private = true }
```

`browser` is the browser's id: its `StartMenuInternet` registry key, or
`custom:<name>` for a browser you added by hand. `crossroads --diagnose`
lists the ids and profiles it found.

| Path                                   | Contents                         |
| -------------------------------------- | -------------------------------- |
| `%APPDATA%\Crossroads\config.toml`     | Rules and settings               |
| `%LOCALAPPDATA%\Crossroads\history.json` | Last 50 routed links           |
| `%LOCALAPPDATA%\Crossroads\icons\`     | Cached browser icons             |

Set `CROSSROADS_CONFIG` and `CROSSROADS_HISTORY` to other files to try
things without touching your real setup.

## How source-app detection works

Most apps open links with `ShellExecute`, which makes the app the parent
process of Crossroads. Some links arrive through a broker process
(`svchost`, `RuntimeBroker`, `explorer`, …), for example from packaged apps.
In that case Crossroads uses the app that owns the foreground window, which
is usually the app you just clicked in. This is a best guess, so check
**Recent links** to see what Crossroads detected.

## Development

```powershell
cargo test                       # rule engine, config, parsing, launch args
cargo clippy --all-targets
cargo run --example gen_icon     # regenerate assets/app.ico
```

CI (`.github/workflows/build.yml`) runs tests, clippy with warnings treated
as errors, and a release build on Windows for every push and pull request,
and uploads the executable as an artifact. When you push a `v*` tag, it also
publishes a GitHub release with `crossroads.exe` attached:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

```
src/
  main.rs, router.rs   mode dispatch and the --url fast path
  ipc.rs               single-instance pipe (Show / Reload)
  model/               GPUI-free core: config, rules, browsers, launch,
                       source detection, registration, history, icons
  ui/                  Fluent theme, title bar, picker, settings pages
```

## Known limitations

- The picker opens centered on the primary monitor, not at the mouse pointer.
- Firefox and Firefox Developer Edition share `profiles.ini`, so both browsers
  list the same profiles.
- Windows only.

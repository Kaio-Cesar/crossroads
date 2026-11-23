//! Windows 11 Fluent design tokens mapped onto the gpui-kit theme.
//!
//! Neutral colors follow the WinUI 3 light/dark resource dictionaries; accent
//! colors come from the user's personalization settings (`UISettings`) and are
//! refreshed live when the user changes them.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

/// Variable Segoe UI on Windows 11; DirectWrite falls back to Segoe UI.
pub const FONT: &str = "Segoe UI Variable Text";
pub const ICON_FONT: &str = "Segoe Fluent Icons";

/// Segoe Fluent Icons code points.
pub mod glyph {
    pub const ADD: &str = "\u{E710}";
    pub const EDIT: &str = "\u{E70F}";
    pub const DELETE: &str = "\u{E74D}";
    pub const FOLDER: &str = "\u{E838}";
    pub const SAVE: &str = "\u{E74E}";
    pub const INFO: &str = "\u{E946}";
    pub const SETTINGS: &str = "\u{E713}";
    pub const FILTER: &str = "\u{E71C}";
    pub const GLOBE: &str = "\u{E774}";
    pub const HISTORY: &str = "\u{E81C}";
    pub const UP: &str = "\u{E70E}";
    pub const DOWN: &str = "\u{E70D}";
    pub const BACK: &str = "\u{E72B}";
    pub const REFRESH: &str = "\u{E72C}";
    pub const LINK: &str = "\u{E71B}";
    pub const LOCK: &str = "\u{E72E}";
    pub const SEARCH: &str = "\u{E721}";
    pub const KEYBOARD: &str = "\u{E765}";
    pub const ROUTE: &str = "\u{E8AB}";
}

/// The system accent color and its six generated shades.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Accent {
    pub base: Rgba,
    pub light1: Rgba,
    pub light2: Rgba,
    pub light3: Rgba,
    pub dark1: Rgba,
    pub dark2: Rgba,
    pub dark3: Rgba,
}

impl Accent {
    /// Windows' default blue, used when `UISettings` is unavailable.
    fn fallback() -> Self {
        Self {
            base: rgb(0x0078D4),
            light1: rgb(0x429CE3),
            light2: rgb(0x76B9ED),
            light3: rgb(0x99EBFF),
            dark1: rgb(0x005FB8),
            dark2: rgb(0x004A83),
            dark3: rgb(0x003A6C),
        }
    }

    #[cfg(windows)]
    pub fn read() -> Self {
        use windows::UI::ViewManagement::{UIColorType, UISettings};
        let read = || -> windows::core::Result<Self> {
            let s = UISettings::new()?;
            let c = |t| -> windows::core::Result<Rgba> {
                let c = s.GetColorValue(t)?;
                Ok(Rgba {
                    r: c.R as f32 / 255.,
                    g: c.G as f32 / 255.,
                    b: c.B as f32 / 255.,
                    a: 1.,
                })
            };
            Ok(Self {
                base: c(UIColorType::Accent)?,
                light1: c(UIColorType::AccentLight1)?,
                light2: c(UIColorType::AccentLight2)?,
                light3: c(UIColorType::AccentLight3)?,
                dark1: c(UIColorType::AccentDark1)?,
                dark2: c(UIColorType::AccentDark2)?,
                dark3: c(UIColorType::AccentDark3)?,
            })
        };
        read().unwrap_or_else(|_| Self::fallback())
    }

    #[cfg(not(windows))]
    pub fn read() -> Self {
        Self::fallback()
    }
}

/// Sets a flag whenever Windows reports a personalization color change.
/// The UI polls [`AccentWatcher::take_changed`] from its timer.
pub struct AccentWatcher {
    changed: Arc<AtomicBool>,
    #[cfg(windows)]
    _settings: Option<(windows::UI::ViewManagement::UISettings, i64)>,
}

impl AccentWatcher {
    pub fn new() -> Self {
        let changed = Arc::new(AtomicBool::new(false));
        #[cfg(windows)]
        {
            use windows::Foundation::TypedEventHandler;
            use windows::UI::ViewManagement::UISettings;
            let flag = changed.clone();
            let settings = UISettings::new().ok().and_then(|s| {
                let token = s
                    .ColorValuesChanged(&TypedEventHandler::new(move |_, _| {
                        flag.store(true, Ordering::Relaxed);
                        Ok(())
                    }))
                    .ok()?;
                Some((s, token))
            });
            Self { changed, _settings: settings }
        }
        #[cfg(not(windows))]
        Self { changed }
    }

    pub fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }
}

#[cfg(windows)]
impl Drop for AccentWatcher {
    fn drop(&mut self) {
        if let Some((s, token)) = &self._settings {
            let _ = s.RemoveColorValuesChanged(*token);
        }
    }
}

/// Fluent palette resolved for the current mode and accent.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub text: Hsla,
    pub text_secondary: Hsla,
    pub text_tertiary: Hsla,
    pub text_on_accent: Hsla,
    pub accent_fill: Hsla,
    pub accent_fill_hover: Hsla,
    pub accent_fill_pressed: Hsla,
    pub accent_text: Hsla,
    /// Background of the content area that sits on Mica ("layer").
    pub layer: Hsla,
    pub card: Hsla,
    pub stroke: Hsla,
    pub control: Hsla,
    pub control_hover: Hsla,
    pub control_pressed: Hsla,
    pub subtle_hover: Hsla,
    pub subtle_pressed: Hsla,
    pub flyout: Hsla,
    /// Solid surface for dialogs/sheets (WinUI ContentDialog background).
    pub surface: Hsla,
    pub success: Hsla,
    pub critical: Hsla,
}

fn a(rgba: u32) -> Hsla {
    gpui_kit::rgba(rgba).into()
}

fn with_alpha(c: Rgba, alpha: f32) -> Hsla {
    Rgba { a: alpha, ..c }.into()
}

impl Palette {
    pub fn new(dark: bool, accent: Accent) -> Self {
        if dark {
            Self {
                text: a(0xFFFFFFFF),
                text_secondary: a(0xFFFFFFC8),
                text_tertiary: a(0xFFFFFF8B),
                text_on_accent: a(0x000000FF),
                accent_fill: accent.light2.into(),
                accent_fill_hover: with_alpha(accent.light2, 0.9),
                accent_fill_pressed: with_alpha(accent.light2, 0.8),
                accent_text: accent.light3.into(),
                layer: a(0x3A3A3A4D),
                card: a(0xFFFFFF0D),
                stroke: a(0xFFFFFF14),
                control: a(0xFFFFFF0F),
                control_hover: a(0xFFFFFF15),
                control_pressed: a(0xFFFFFF08),
                subtle_hover: a(0xFFFFFF0F),
                subtle_pressed: a(0xFFFFFF0B),
                flyout: a(0x2C2C2CFF),
                surface: a(0x2B2B2BFF),
                success: a(0x6CCB5FFF),
                critical: a(0xFF99A4FF),
            }
        } else {
            Self {
                text: a(0x000000E4),
                text_secondary: a(0x0000009E),
                text_tertiary: a(0x00000072),
                text_on_accent: a(0xFFFFFFFF),
                accent_fill: accent.dark1.into(),
                accent_fill_hover: with_alpha(accent.dark1, 0.9),
                accent_fill_pressed: with_alpha(accent.dark1, 0.8),
                accent_text: accent.dark2.into(),
                layer: a(0xFFFFFF80),
                card: a(0xFFFFFFB3),
                stroke: a(0x0000000F),
                control: a(0xFFFFFFB3),
                control_hover: a(0xF9F9F980),
                control_pressed: a(0xF9F9F94D),
                subtle_hover: a(0x0000000A),
                subtle_pressed: a(0x00000006),
                flyout: a(0xF9F9F9FF),
                surface: a(0xFFFFFFFF),
                success: a(0x0F7B0FFF),
                critical: a(0xC42B1CFF),
            }
        }
    }
}

impl Global for Palette {}

/// Re-reads light/dark + accent and applies the Fluent palette to the theme.
pub fn apply(window: &mut Window, cx: &mut App) {
    let dark = matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark);
    let mode = if dark { ThemeMode::Dark } else { ThemeMode::Light };
    Theme::change(mode, Some(window), cx);

    let p = Palette::new(dark, Accent::read());
    cx.set_global(p);
    override_theme(p, cx);
    window.refresh();
}

fn override_theme(p: Palette, cx: &mut App) {
    let dark = Theme::global(cx).is_dark();
    // `Theme::update` (unlike `global_mut`) reconciles the semantic tokens with
    // `colors` and syncs gpui-base's copy. The root surface paints
    // `tokens.background`, so without this it stays opaque over Mica.
    Theme::update(cx, |theme| {
        // WinUI TextBox fill: translucent, so Mica still tints the editor.
        let mut highlight = (*theme.highlight_theme).clone();
        highlight.style.editor_background =
            Some(if dark { a(0x1E1E1EB3) } else { a(0xFFFFFFB3) });
        theme.highlight_theme = std::sync::Arc::new(highlight);

        theme.font_family = FONT.into();
        theme.font_size = px(14.);
        theme.radius = px(4.);
        theme.radius_lg = px(8.);
        theme.shadow = false;

        let clear = gpui_kit::transparent_black();
        let c = &mut theme.colors;
        c.background = p.surface;
        c.foreground = p.text;
        c.muted = p.subtle_hover;
        c.muted_foreground = p.text_secondary;
        c.border = p.stroke;
        c.input = if dark { a(0xFFFFFF1F) } else { a(0x00000029) };
        c.ring = p.accent_fill;
        c.caret = p.text;
        c.selection = p.accent_fill.opacity(0.45);
        c.accent = p.subtle_hover;
        c.accent_foreground = p.text;
        c.overlay = a(0x0000004D);
        c.popover = p.flyout;
        c.popover_foreground = p.text;
        c.window_border = p.stroke;
        c.link = p.accent_text;
        c.link_hover = p.accent_text;
        c.link_active = p.accent_text;

        c.primary = p.accent_fill;
        c.primary_hover = p.accent_fill_hover;
        c.primary_active = p.accent_fill_pressed;
        c.primary_foreground = p.text_on_accent;
        c.button_primary = p.accent_fill;
        c.button_primary_hover = p.accent_fill_hover;
        c.button_primary_active = p.accent_fill_pressed;
        c.button_primary_foreground = p.text_on_accent;

        c.secondary = p.control;
        c.secondary_hover = p.control_hover;
        c.secondary_active = p.control_pressed;
        c.secondary_foreground = p.text;
        c.button = p.control;
        c.button_hover = p.control_hover;
        c.button_active = p.control_pressed;
        c.button_foreground = p.text;
        c.button_secondary = p.control;
        c.button_secondary_hover = p.control_hover;
        c.button_secondary_active = p.control_pressed;
        c.button_secondary_foreground = p.text;

        c.danger = p.critical;
        c.danger_hover = p.critical.opacity(0.9);
        c.danger_active = p.critical.opacity(0.8);
        c.danger_foreground = p.text_on_accent;
        c.button_danger = p.critical;
        c.button_danger_hover = p.critical.opacity(0.9);
        c.button_danger_active = p.critical.opacity(0.8);
        c.button_danger_foreground = p.text_on_accent;
        c.success = p.success;

        c.list = clear;
        c.list_hover = p.subtle_hover;
        c.list_active = p.subtle_hover;
        c.list_active_border = p.accent_fill;
        c.sidebar = clear;
        c.title_bar = clear;
        c.title_bar_border = clear;
        c.scrollbar = clear;
        c.switch = p.control;
        c.tab_bar = clear;

    });
    cx.refresh_windows();
}

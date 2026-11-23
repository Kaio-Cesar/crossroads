//! Small Fluent building blocks shared by the settings window and picker.

use std::collections::HashMap;
use std::path::PathBuf;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;

use super::fluent::{ICON_FONT, Palette, glyph};

/// A Segoe Fluent Icons glyph.
pub fn icon(g: &'static str) -> Div {
    div().font_family(ICON_FONT).text_size(px(16.)).line_height(px(16.)).child(g)
}

/// Glyph + label, for buttons.
pub fn icon_label(g: &'static str, label: impl Into<SharedString>) -> impl IntoElement {
    h_flex().gap_2().items_center().child(icon(g).text_size(px(14.))).child(label.into())
}

/// Settings-style card ("SettingsCard"): leading visual, header + description, trailing content.
pub fn card_with(p: &Palette, leading: impl IntoElement, header: impl IntoElement, description: impl IntoElement) -> Div {
    h_flex()
        .w_full()
        .min_h(px(68.))
        .px_4()
        .py_3()
        .gap_4()
        .items_center()
        .rounded(px(8.))
        .bg(p.card)
        .border_1()
        .border_color(p.stroke)
        .child(leading)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(div().child(header))
                .child(div().text_xs().text_color(p.text_secondary).child(description)),
        )
}

/// [`card_with`] with a glyph as the leading visual.
pub fn card(p: &Palette, g: &'static str, header: impl IntoElement, description: impl IntoElement) -> Div {
    card_with(p, icon(g).text_size(px(20.)).w(px(20.)).text_color(p.text), header, description)
}

/// Section heading above a group of cards.
pub fn section(text: impl Into<SharedString>) -> Div {
    div().pt_4().pb_1().text_sm().font_weight(FontWeight::SEMIBOLD).child(text.into())
}

/// Page title (WinUI "TitleLarge"-ish).
pub fn page_title(text: impl Into<SharedString>) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_size(px(28.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.into())
}

/// Fluent ContentDialog title: 20px semibold.
pub fn dialog_title(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_size(px(20.))
        .line_height(px(28.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.into())
}

/// Fluent dialog footer: primary action + Cancel, equal widths.
pub fn dialog_footer(
    ok: &'static str,
    danger: bool,
    on_ok: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    use gpui_kit::component::WindowExt as _;
    let ok_button = Button::new("dialog-ok").label(ok).flex_1();
    let ok_button = if danger { ok_button.danger() } else { ok_button.primary() };
    h_flex()
        .w_full()
        .justify_end()
        .gap_2()
        .child(ok_button.on_click(move |_, window, cx| on_ok(window, cx)))
        .child(
            Button::new("dialog-cancel")
                .label("Cancel")
                .flex_1()
                .on_click(|_, window, cx| window.close_dialog(cx)),
        )
}

/// Small rounded label, e.g. a condition in a rule summary.
pub fn chip(p: &Palette, text: impl Into<SharedString>) -> Div {
    div()
        .px_2()
        .py(px(1.))
        .rounded(px(4.))
        .bg(p.subtle_hover)
        .border_1()
        .border_color(p.stroke)
        .text_xs()
        .text_color(p.text_secondary)
        .max_w(px(260.))
        .truncate()
        .child(text.into())
}

/// Inline error/warning line.
pub fn error_line(p: &Palette, text: impl Into<SharedString>) -> Div {
    h_flex()
        .gap_2()
        .items_center()
        .text_xs()
        .text_color(p.critical)
        .child(icon(glyph::INFO).text_size(px(12.)))
        .child(text.into())
}

/// A browser's icon (by browser id), or a globe until it has been extracted.
pub fn browser_icon(icons: &HashMap<String, PathBuf>, browser: &str, size: Pixels, p: &Palette) -> AnyElement {
    match icons.get(browser) {
        Some(path) => img(path.clone()).size(size).flex_shrink_0().into_any_element(),
        None => icon(glyph::GLOBE)
            .text_size(size * 0.8)
            .w(size)
            .flex_shrink_0()
            .text_color(p.text_secondary)
            .into_any_element(),
    }
}

/// Extracts icons for `browsers` (cached on disk after the first time).
pub fn load_icons(browsers: &[crate::model::browsers::Browser]) -> HashMap<String, PathBuf> {
    #[cfg(windows)]
    {
        browsers
            .iter()
            .filter_map(|b| match crate::model::icons::ensure(&b.exe) {
                Ok(path) => Some((b.id.clone(), path)),
                Err(e) => {
                    eprintln!("icon for {}: {e:#}", b.name);
                    None
                }
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        let _ = browsers;
        HashMap::new()
    }
}

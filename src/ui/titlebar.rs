//! Windows 11-style title bar drawn into the Mica client area.
//!
//! Caption buttons use Segoe Fluent Icons glyphs and native hit-testing
//! (`WindowControlArea`), so Snap Layouts appear on the maximize button.

use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::fluent::{ICON_FONT, Palette};

pub const HEIGHT: Pixels = px(48.);
const BUTTON_WIDTH: Pixels = px(46.);

#[derive(IntoElement)]
pub struct FluentTitleBar {
    children: Vec<AnyElement>,
    close_only: bool,
}

impl FluentTitleBar {
    pub fn new() -> Self {
        Self { children: Vec::new(), close_only: false }
    }

    /// Only the close button (for small fixed-size windows like the picker).
    pub fn close_only(mut self) -> Self {
        self.close_only = true;
        self
    }
}

impl ParentElement for FluentTitleBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

#[derive(Clone, Copy)]
enum Caption {
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl Caption {
    fn glyph(self) -> &'static str {
        match self {
            Self::Minimize => "\u{E921}",
            Self::Maximize => "\u{E922}",
            Self::Restore => "\u{E923}",
            Self::Close => "\u{E8BB}",
        }
    }

    fn area(self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Maximize | Self::Restore => WindowControlArea::Max,
            Self::Close => WindowControlArea::Close,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Minimize => "caption-min",
            Self::Maximize | Self::Restore => "caption-max",
            Self::Close => "caption-close",
        }
    }
}

fn caption_button(kind: Caption, p: &Palette, active: bool) -> impl IntoElement {
    let close = matches!(kind, Caption::Close);
    let close_bg: Hsla = rgb(0xC42B1C).into();
    h_flex()
        .id(kind.id())
        .w(BUTTON_WIDTH)
        .h_full()
        .flex_shrink_0()
        .justify_center()
        .items_center()
        .font_family(ICON_FONT)
        .text_size(px(10.))
        .text_color(if active { p.text } else { p.text_tertiary })
        .window_control_area(kind.area())
        .when(close, |d| {
            d.hover(move |s| s.bg(close_bg).text_color(white()))
                .active(move |s| s.bg(close_bg.opacity(0.9)).text_color(white()))
        })
        .when(!close, |d| {
            let (hover, pressed) = (p.subtle_hover, p.subtle_pressed);
            d.hover(move |s| s.bg(hover)).active(move |s| s.bg(pressed))
        })
        .child(kind.glyph())
}

impl RenderOnce for FluentTitleBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *cx.global::<Palette>();
        let active = window.is_window_active();
        let max = if window.is_maximized() { Caption::Restore } else { Caption::Maximize };
        h_flex()
            .id("title-bar")
            .w_full()
            .h(HEIGHT)
            .flex_shrink_0()
            .items_center()
            .child(
                h_flex()
                    .id("title-drag")
                    .flex_1()
                    .h_full()
                    .pl_4()
                    .items_center()
                    .window_control_area(WindowControlArea::Drag)
                    .when(!active, |d| d.text_color(p.text_tertiary))
                    .children(self.children),
            )
            .when(!self.close_only, |d| {
                d.child(caption_button(Caption::Minimize, &p, active))
                    .child(caption_button(max, &p, active))
            })
            .child(caption_button(Caption::Close, &p, active))
    }
}

//! The "open with…" picker: shown when no rule decides, when the fallback is
//! "ask", or when Shift was held. Keyboard first: 1–9 pick, arrows + Enter,
//! Ctrl for a private window, Esc to cancel.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::fluent::{self, AccentWatcher, Palette, glyph};
use super::titlebar::FluentTitleBar;
use super::widgets::{browser_icon, icon, load_icons};
use crate::model::browsers::{self, Browser, TargetOption};
use crate::model::config::{Config, Matcher, Rule, Target};
use crate::model::rules::{self, LinkContext};
use crate::model::{history, launch};
use crate::router::PickRequest;

const WIDTH: f32 = 420.;
const ROW: f32 = 44.;

pub fn run(request: PickRequest) {
    gpui_kit::application().with_assets(gpui_kit::assets::Assets).run(move |cx| {
        gpui_kit::init(cx);
        let icons = load_icons(&request.browsers);
        let rows = browsers::target_options(&request.browsers, &request.config).len().max(1) as f32;
        // Title bar + link header + rows + footer, capped (the list scrolls).
        let height = (48. + 76. + rows * (ROW + 2.) + 96.).min(640.);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(WIDTH), px(height)), cx)),
            window_background: WindowBackgroundAppearance::MicaBackdrop,
            titlebar: Some(TitlebarOptions {
                title: Some("Open link with".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            app_owns_titlebar_drag: true,
            is_resizable: false,
            is_minimizable: false,
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            fluent::apply(window, cx);
            let view = cx.new(|cx| PickerView::new(request, icons, window, cx));
            cx.new(|cx| base::Root::new(view, window, cx).bg(transparent_black()))
        })
        .expect("failed to open picker window");
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
}

pub struct PickerView {
    ctx: LinkContext,
    browsers: Vec<Browser>,
    options: Vec<TargetOption>,
    icons: HashMap<String, PathBuf>,
    host: Option<String>,
    selected: usize,
    private: bool,
    remember: bool,
    focus: FocusHandle,
    accent: AccentWatcher,
    _subscriptions: Vec<Subscription>,
    _ticker: Task<()>,
}

impl PickerView {
    fn new(request: PickRequest, icons: HashMap<String, PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let options = browsers::target_options(&request.browsers, &request.config);
        // Preselect the default target if it's listed.
        let selected = request
            .config
            .default_target
            .as_ref()
            .and_then(|d| {
                let same_browser = |o: &&TargetOption| o.target.browser == d.browser;
                options
                    .iter()
                    .position(|o| same_browser(&o) && o.target.profile == d.profile)
                    .or_else(|| options.iter().position(|o| same_browser(&o)))
            })
            .unwrap_or(0);
        let focus = cx.focus_handle();
        focus.focus(window, cx);

        let ticker = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(500)).await;
                let alive = this.update_in(cx, |this, window, cx| {
                    if this.accent.take_changed() {
                        fluent::apply(window, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });

        Self {
            host: rules::host_of(&request.ctx.url).map(|h| h.trim_start_matches("www.").to_string()),
            ctx: request.ctx,
            browsers: request.browsers,
            options,
            icons,
            selected,
            private: false,
            remember: false,
            focus,
            accent: AccentWatcher::new(),
            _subscriptions: vec![cx.observe_window_appearance(window, |_, window, cx| fluent::apply(window, cx))],
            _ticker: ticker,
        }
    }

    fn choose(&mut self, index: usize, private: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(option) = self.options.get(index) else { return };
        let target = Target { private: private && self.supports_private(index), ..option.target.clone() };
        if let Err(e) = launch::open(&self.browsers, &target, &self.ctx.url) {
            window.push_notification(Notification::error(format!("{e:#}")), cx);
            return;
        }
        let mut rule_id = None;
        if self.remember
            && let Some(host) = &self.host
        {
            match remember(host, &target) {
                Ok(id) => rule_id = Some(id),
                Err(e) => eprintln!("saving rule: {e:#}"),
            }
        }
        let entry = history::Entry::now(&self.ctx.url, self.ctx.source_exe.clone(), Some(target), rule_id);
        if let Err(e) = history::push(&history::default_path(), entry) {
            eprintln!("history: {e:#}");
        }
        cx.quit();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        let entry = history::Entry::now(&self.ctx.url, self.ctx.source_exe.clone(), None, None);
        let _ = history::push(&history::default_path(), entry);
        cx.quit();
    }

    fn supports_private(&self, index: usize) -> bool {
        self.options
            .get(index)
            .and_then(|o| browsers::find(&self.browsers, &o.target.browser))
            .is_some_and(|b| b.supports_private())
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let private = self.private || ks.modifiers.control;
        let n = self.options.len();
        match ks.key.as_str() {
            "escape" => self.cancel(cx),
            "enter" => self.choose(self.selected, private, window, cx),
            "down" | "tab" if n > 0 && !ks.modifiers.shift => self.selected = (self.selected + 1) % n,
            "up" | "tab" if n > 0 => self.selected = (self.selected + n - 1) % n,
            key => {
                if let Some(d) = key.parse::<usize>().ok().filter(|d| (1..=9).contains(d)) {
                    self.choose(d - 1, private, window, cx);
                }
                return;
            }
        }
        cx.notify();
    }

    fn render_option(&self, index: usize, option: &TargetOption, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = index == self.selected;
        h_flex()
            .id(SharedString::from(format!("option-{index}")))
            .relative()
            .h(px(ROW))
            .px_3()
            .gap_3()
            .items_center()
            .rounded(px(4.))
            .when(selected, |d| d.bg(p.subtle_hover))
            .hover(|d| d.bg(p.subtle_hover))
            .active(|d| d.bg(p.subtle_pressed))
            .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                let private = this.private || ev.modifiers().control;
                this.choose(index, private, window, cx)
            }))
            .when(selected, |d| {
                d.child(div().absolute().left_0().top(px(14.)).w(px(3.)).h(px(16.)).rounded(px(2.)).bg(p.accent_fill))
            })
            .child(browser_icon(&self.icons, &option.target.browser, px(24.), p))
            .child(div().flex_1().min_w_0().truncate().child(option.label.clone()))
            .when(index < 9, |d| {
                d.child(
                    div()
                        .w(px(20.))
                        .h(px(20.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(p.stroke)
                        .text_xs()
                        .text_color(p.text_tertiary)
                        .child(format!("{}", index + 1)),
                )
            })
    }
}

/// Saves "always open `host` in `target`" as the first rule, so it wins
/// over broader rules. Returns the new rule's id.
fn remember(host: &str, target: &Target) -> anyhow::Result<String> {
    let path = Config::default_path();
    let mut config = Config::load(&path)?;
    let id = config.new_rule_id();
    config.rules.insert(
        0,
        Rule {
            id: id.clone(),
            name: host.to_string(),
            enabled: true,
            matchers: vec![Matcher::Domain(host.to_string())],
            target: target.clone(),
        },
    );
    config.save(&path)?;
    // Let an open settings window pick up the new rule.
    crate::ipc::forward(&crate::ipc::Request::Reload);
    Ok(id)
}

impl Render for PickerView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *cx.global::<Palette>();
        let options: Vec<_> = self.options.iter().enumerate().map(|(i, o)| self.render_option(i, o, &p, cx).into_any_element()).collect();
        let empty = options.is_empty();
        let any_private = (0..self.options.len()).any(|i| self.supports_private(i));

        let link = v_flex()
            .px_4()
            .pb_3()
            .gap_0p5()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(icon(glyph::LINK).text_color(p.accent_text))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(16.))
                            .child(self.host.clone().unwrap_or_else(|| "Link".into())),
                    ),
            )
            .child(div().text_xs().text_color(p.text_secondary).truncate().child(self.ctx.url.clone()))
            .when_some(self.ctx.source_exe.clone(), |d, source| {
                d.child(div().text_xs().text_color(p.text_tertiary).child(format!("Opened from {source}")))
            });

        let list = v_flex()
            .id("options")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_2()
            .gap(px(2.))
            .children(options)
            .when(empty, |d| {
                d.child(
                    div()
                        .p_4()
                        .text_color(p.text_secondary)
                        .child("No browsers found. Open Crossroads settings to add one."),
                )
            });

        let footer = v_flex()
            .px_4()
            .py_3()
            .gap_2()
            .border_t_1()
            .border_color(p.stroke)
            .bg(p.layer)
            .when(any_private, |d| {
                d.child(
                    Checkbox::new("private")
                        .label("Private window (Ctrl)")
                        .checked(self.private)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.private = *checked;
                            cx.notify();
                        })),
                )
            })
            .when_some(self.host.clone(), |d, host| {
                d.child(
                    Checkbox::new("remember")
                        .label(format!("Always open {host} there"))
                        .checked(self.remember)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.remember = *checked;
                            cx.notify();
                        })),
                )
            });

        v_flex()
            .id("picker")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .size_full()
            .font_family(fluent::FONT)
            .text_size(px(14.))
            .text_color(p.text)
            .child(
                FluentTitleBar::new().close_only().child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .child(icon(glyph::ROUTE).text_size(px(14.)).text_color(p.accent_fill))
                        .child(div().text_xs().child("Open link with")),
                ),
            )
            .child(link)
            .child(list)
            .child(footer)
    }
}

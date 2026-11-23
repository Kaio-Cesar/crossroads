//! Settings window: a Settings-style navigation list (General, Rules,
//! Browsers, History) and the selected page.

mod pages;
mod rules;

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::button::Button;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::fluent::{self, AccentWatcher, Palette, glyph};
use super::titlebar::FluentTitleBar;
use super::widgets::{browser_icon, icon, load_icons};
use crate::ipc::Request;
use crate::model::browsers::{self, Browser};
use crate::model::config::{Config, Target};
use crate::model::registration::{self, Status};
use crate::model::history;

pub use rules::RuleEditor;

pub fn run(ipc: Option<async_channel::Receiver<Request>>) {
    gpui_kit::application().with_assets(gpui_kit::assets::Assets).run(move |cx| {
        gpui_kit::init(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1000.), px(740.)), cx)),
            window_min_size: Some(size(px(760.), px(500.))),
            window_background: WindowBackgroundAppearance::MicaBackdrop,
            titlebar: Some(TitlebarOptions {
                title: Some("Crossroads".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            app_owns_titlebar_drag: true,
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            fluent::apply(window, cx);
            let view = cx.new(|cx| SettingsView::new(ipc, window, cx));
            // Only the window surface is transparent (so Mica shows through);
            // the theme background stays solid for dialogs and other overlays.
            cx.new(|cx| base::Root::new(view, window, cx).bg(transparent_black()))
        })
        .expect("failed to open window");
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    General,
    Rules,
    Browsers,
    History,
}

impl Page {
    const ALL: [Self; 4] = [Self::General, Self::Rules, Self::Browsers, Self::History];

    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Rules => "Rules",
            Self::Browsers => "Browsers",
            Self::History => "Recent links",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::General => glyph::SETTINGS,
            Self::Rules => glyph::FILTER,
            Self::Browsers => glyph::GLOBE,
            Self::History => glyph::HISTORY,
        }
    }
}

/// Re-check registration (the user may change the default in Settings) this often.
const TICK: Duration = Duration::from_millis(1000);

pub struct SettingsView {
    config_path: PathBuf,
    config: Config,
    /// Set when `config.toml` couldn't be parsed; saving is blocked so the
    /// broken file isn't overwritten.
    config_error: Option<String>,
    browsers: Vec<Browser>,
    icons: HashMap<String, PathBuf>,
    page: Page,
    editor: Option<RuleEditor>,
    history: Vec<history::Entry>,
    status: Status,
    is_default: bool,
    test_url: Entity<InputState>,
    test_source: Entity<InputState>,
    accent: AccentWatcher,
    _subscriptions: Vec<Subscription>,
    _ticker: Task<()>,
    _ipc_task: Option<Task<()>>,
    _icon_task: Option<Task<()>>,
}

impl SettingsView {
    fn new(ipc: Option<async_channel::Receiver<Request>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let test_url = cx.new(|cx| InputState::new(window, cx).placeholder("Paste a link, e.g. https://github.com/rust-lang"));
        let test_source = cx.new(|cx| InputState::new(window, cx).placeholder("Opened from (optional), e.g. slack.exe"));
        let notify_on_change = |cx: &mut Context<Self>, input: &Entity<InputState>, window: &mut Window| {
            cx.subscribe_in(input, window, |_, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    cx.notify();
                }
            })
        };
        let subscriptions = vec![
            notify_on_change(cx, &test_url, window),
            notify_on_change(cx, &test_source, window),
            cx.observe_window_appearance(window, |_, window, cx| fluent::apply(window, cx)),
        ];

        let ticker = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let alive = this.update_in(cx, |this, window, cx| {
                    if this.accent.take_changed() {
                        fluent::apply(window, cx);
                    }
                    this.refresh_registration(cx);
                });
                if alive.is_err() {
                    break;
                }
            }
        });

        let ipc_task = ipc.map(|requests| {
            cx.spawn_in(window, async move |this, cx| {
                while let Ok(request) = requests.recv().await {
                    let alive = this.update_in(cx, |this, window, cx| match request {
                        Request::Show => window.activate_window(),
                        Request::Reload => this.reload(window, cx),
                    });
                    if alive.is_err() {
                        break;
                    }
                }
            })
        });

        let exe = registration::current_exe();
        let mut status = registration::status(&exe);
        // The exe moved (rebuilt elsewhere, reinstalled): point Windows at this one.
        if status == Status::Outdated {
            match registration::register(&exe) {
                Ok(()) => status = Status::Registered,
                Err(e) => eprintln!("re-registering: {e}"),
            }
        }

        let mut this = Self {
            config_path: Config::default_path(),
            config: Config::default(),
            config_error: None,
            browsers: Vec::new(),
            icons: HashMap::new(),
            page: Page::General,
            editor: None,
            history: Vec::new(),
            status,
            is_default: registration::is_default(),
            test_url,
            test_source,
            accent: AccentWatcher::new(),
            _subscriptions: subscriptions,
            _ticker: ticker,
            _ipc_task: ipc_task,
            _icon_task: None,
        };
        this.reload(window, cx);
        // Debug builds: start on another page (for screenshots), e.g. `CROSSROADS_PAGE=rules`.
        #[cfg(debug_assertions)]
        if let Ok(page) = std::env::var("CROSSROADS_PAGE") {
            match page.as_str() {
                "rules" => this.page = Page::Rules,
                "browsers" => this.page = Page::Browsers,
                "history" => this.page = Page::History,
                "editor" => {
                    this.page = Page::Rules;
                    let first = this.config.rules.first().map(|r| r.id.clone());
                    this.open_editor(first, Vec::new(), None, window, cx);
                }
                _ => {}
            }
        }
        this
    }

    /// Re-reads the config, rediscovers browsers and refreshes their icons.
    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match Config::load(&self.config_path) {
            Ok(config) => {
                self.config = config;
                self.config_error = None;
            }
            Err(e) => self.config_error = Some(format!("{e:#}")),
        }
        self.browsers = browsers::discover(&self.config);
        self.history = history::load(&history::default_path());
        let browsers = self.browsers.clone();
        self._icon_task = Some(cx.spawn_in(window, async move |this, cx| {
            let icons = cx.background_executor().spawn(async move { load_icons(&browsers) }).await;
            let _ = this.update(cx, |this, cx| {
                this.icons = icons;
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn refresh_registration(&mut self, cx: &mut Context<Self>) {
        let status = registration::status(&registration::current_exe());
        let is_default = registration::is_default();
        if (status, is_default) != (self.status, self.is_default) {
            self.status = status;
            self.is_default = is_default;
            cx.notify();
        }
    }

    /// Applies `change` to the config and saves it. Returns `false` (and
    /// shows why) if it couldn't be saved.
    fn update_config(&mut self, window: &mut Window, cx: &mut Context<Self>, change: impl FnOnce(&mut Config)) -> bool {
        if let Some(e) = &self.config_error {
            window.push_notification(
                Notification::error(format!("config.toml has errors; fix or delete it first.\n{e}")),
                cx,
            );
            return false;
        }
        let before = self.config.clone();
        change(&mut self.config);
        if let Err(e) = self.config.save(&self.config_path) {
            self.config = before;
            window.push_notification(Notification::error(format!("Couldn't save settings: {e:#}")), cx);
            return false;
        }
        cx.notify();
        true
    }

    fn select_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        self.editor = None;
        if page == Page::History {
            self.history = history::load(&history::default_path());
        }
        cx.notify();
    }

    fn label(&self, target: &Target) -> String {
        browsers::target_label(&self.browsers, target)
    }

    /// A dropdown button listing every visible target; `on_pick` gets the choice.
    fn target_picker(
        &self,
        id: &'static str,
        current: Option<&Target>,
        p: &Palette,
        on_pick: impl Fn(&mut Self, Target, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let options = browsers::target_options(&self.browsers, &self.config);
        let label = current.map(|t| self.label(&Target { private: false, ..t.clone() })).unwrap_or_else(|| "Choose a browser…".into());
        let current = current.map(|t| (t.browser.clone(), t.profile.clone()));
        let view = cx.entity();
        let on_pick = Rc::new(on_pick);
        Button::new(id)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .when_some(current.clone(), |d, (browser, _)| d.child(browser_icon(&self.icons, &browser, px(16.), p)))
                    .child(div().max_w(px(260.)).truncate().child(label))
                    .child(icon(glyph::DOWN).text_size(px(10.)).text_color(p.text_secondary)),
            )
            .dropdown_menu(move |menu, _, _| {
                options.iter().fold(menu.max_h(px(420.)).scrollable(true), |menu, option| {
                    let view = view.clone();
                    let on_pick = on_pick.clone();
                    let target = option.target.clone();
                    let checked = current.as_ref() == Some(&(target.browser.clone(), target.profile.clone()));
                    menu.item(PopupMenuItem::new(option.label.clone()).checked(checked).on_click(move |_, window, cx| {
                        let target = target.clone();
                        let on_pick = on_pick.clone();
                        view.update(cx, |this, cx| on_pick(this, target, window, cx));
                    }))
                })
            })
    }

    fn render_nav(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let items = Page::ALL.into_iter().map(|page| {
            let selected = page == self.page;
            h_flex()
                .id(SharedString::from(format!("nav-{}", page.label())))
                .relative()
                .h(px(36.))
                .px_3()
                .gap_3()
                .items_center()
                .rounded(px(4.))
                .when(selected, |d| d.bg(p.subtle_hover))
                .hover(|d| d.bg(p.subtle_hover))
                .active(|d| d.bg(p.subtle_pressed))
                .on_click(cx.listener(move |this, _, _, cx| this.select_page(page, cx)))
                .when(selected, |d| {
                    // Fluent selection indicator pill.
                    d.child(div().absolute().left_0().top(px(10.)).w(px(3.)).h(px(16.)).rounded(px(2.)).bg(p.accent_fill))
                })
                .child(icon(page.glyph()))
                .child(div().flex_1().child(page.label()))
                .when(page == Page::Rules && !self.config.rules.is_empty(), |d| {
                    d.child(div().text_xs().text_color(p.text_tertiary).child(self.config.rules.len().to_string()))
                })
        });
        v_flex().w(px(240.)).flex_shrink_0().px_1().pb_2().gap_1().children(items)
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *cx.global::<Palette>();
        let page = match (&self.editor, self.page) {
            (Some(_), _) => self.render_editor(&p, window, cx).into_any_element(),
            (None, Page::General) => self.render_general(&p, cx).into_any_element(),
            (None, Page::Rules) => self.render_rules(&p, cx).into_any_element(),
            (None, Page::Browsers) => self.render_browsers(&p, cx).into_any_element(),
            (None, Page::History) => self.render_history(&p, cx).into_any_element(),
        };
        v_flex()
            .size_full()
            .font_family(fluent::FONT)
            .text_size(px(14.))
            .text_color(p.text)
            .child(
                FluentTitleBar::new().child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .child(icon(glyph::ROUTE).text_size(px(14.)).text_color(p.accent_fill))
                        .child(div().text_xs().child("Crossroads")),
                ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .child(self.render_nav(&p, cx))
                    .child(
                        v_flex()
                            .id("page")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_y_scroll()
                            .bg(p.layer)
                            .border_t_1()
                            .border_l_1()
                            .border_color(p.stroke)
                            .rounded_tl(px(8.))
                            .px(px(36.))
                            .pt_6()
                            .pb_8()
                            .gap_1()
                            .when_some(self.config_error.clone(), |d, e| {
                                d.child(
                                    super::widgets::card(&p, glyph::INFO, "config.toml couldn't be read", e)
                                        .border_color(p.critical)
                                        .mb_2(),
                                )
                            })
                            .child(page),
                    ),
            )
    }
}

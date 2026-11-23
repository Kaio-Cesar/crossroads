//! General, Browsers and Recent links pages.

use std::cell::Cell;
use std::rc::Rc;

/// A shared window action (dialog submit handlers).
type Callback = Rc<dyn Fn(&mut Window, &mut App)>;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::super::fluent::{Palette, glyph};
use super::super::widgets::{browser_icon, card, card_with, chip, dialog_footer, dialog_title, icon, icon_label, page_title, section};
use super::{Page, SettingsView};
use crate::model::browsers::Family;
use crate::model::config::{CustomBrowser, Fallback, Matcher};
use crate::model::registration::{self, Status};
use crate::model::{history, rules, source};

/// "5 min ago"-style relative time.
fn ago(time: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(time);
    match now.saturating_sub(time) {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86400 => format!("{} h ago", s / 3600),
        s => format!("{} d ago", s / 86400),
    }
}

impl SettingsView {
    fn register(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match registration::register(&registration::current_exe()) {
            Ok(()) => {
                self.refresh_registration(cx);
                let _ = registration::open_default_apps_settings();
            }
            Err(e) => window.push_notification(Notification::error(format!("Couldn't register: {e}")), cx),
        }
    }

    fn unregister(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = registration::unregister() {
            window.push_notification(Notification::error(format!("Couldn't unregister: {e}")), cx);
        }
        self.refresh_registration(cx);
    }

    pub(super) fn render_general(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let (header, description) = match (self.status, self.is_default) {
            (Status::Registered, true) => ("Crossroads is your default browser", "Links you open anywhere in Windows go through your rules"),
            (Status::Registered, false) => (
                "Almost there: make Crossroads the default",
                "In Settings › Default apps › Crossroads, choose “Set default”",
            ),
            _ => ("Not set up yet", "Register Crossroads as a browser, then make it the default in Windows Settings"),
        };
        let registered = self.status == Status::Registered;
        let handler = card(p, glyph::ROUTE, header, description)
            .child(
                div()
                    .size(px(10.))
                    .rounded_full()
                    .when(registered && self.is_default, |d| d.bg(p.success))
                    .when(!(registered && self.is_default), |d| d.border_1().border_color(p.text_tertiary)),
            )
            .when(registered, |d| {
                d.child(
                    Button::new("unregister")
                        .ghost()
                        .label("Unregister")
                        .on_click(cx.listener(|this, _, window, cx| this.unregister(window, cx))),
                )
                .when(!self.is_default, |d| {
                    d.child(Button::new("open-settings").primary().label("Open Default apps").on_click(|_, window, cx| {
                        if let Err(e) = registration::open_default_apps_settings() {
                            window.push_notification(Notification::error(e.to_string()), cx);
                        }
                    }))
                })
            })
            .when(!registered, |d| {
                d.child(
                    Button::new("register")
                        .primary()
                        .label("Set up")
                        .on_click(cx.listener(|this, _, window, cx| this.register(window, cx))),
                )
            });

        let view = cx.entity();
        let fallback_label = match self.config.fallback {
            Fallback::Default => "Open in the default browser",
            Fallback::Picker => "Ask me every time",
        };
        let fallback = card(p, glyph::FILTER, "When no rule matches", "Links that none of your rules catch").child(
            Button::new("fallback")
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(fallback_label)
                        .child(icon(glyph::DOWN).text_size(px(10.)).text_color(p.text_secondary)),
                )
                .dropdown_menu(move |menu, _, _| {
                    [(Fallback::Default, "Open in the default browser"), (Fallback::Picker, "Ask me every time")]
                        .into_iter()
                        .fold(menu, |menu, (value, label)| {
                            let view = view.clone();
                            menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.update_config(window, cx, |c| c.fallback = value);
                                })
                            }))
                        })
                }),
        );

        let picks_default = self.config.fallback == Fallback::Default;
        let default_target = card(
            p,
            glyph::GLOBE,
            "Default browser",
            if picks_default {
                "Where unmatched links open"
            } else {
                "Preselected in the picker"
            },
        )
        .child(self.target_picker(
            "default-target",
            self.config.default_target.as_ref(),
            p,
            |this, target, window, cx| {
                this.update_config(window, cx, |c| c.default_target = Some(target));
            },
            cx,
        ));

        let shift = card(
            p,
            glyph::KEYBOARD,
            "Choose each time with Shift",
            "Hold Shift while clicking a link to pick the browser yourself, whatever the rules say",
        );

        let folder = card(p, glyph::FOLDER, "Settings file", self.config_path.display().to_string()).child(
            Button::new("open-config")
                .label("Open folder")
                .on_click(cx.listener(|this, _, window, cx| {
                    let dir = this.config_path.parent().map(|d| d.to_path_buf()).unwrap_or_default();
                    if let Err(e) = std::fs::create_dir_all(&dir)
                        .and_then(|_| std::process::Command::new("explorer").arg(&dir).spawn().map(|_| ()))
                    {
                        window.push_notification(Notification::error(e.to_string()), cx);
                    }
                })),
        );

        v_flex()
            .gap_1()
            .child(h_flex().child(page_title("General")))
            .child(section("Setup"))
            .child(handler)
            .child(section("Routing"))
            .child(fallback)
            .child(default_target)
            .child(shift)
            .child(section("About"))
            .child(folder)
    }

    // ---- browsers ------------------------------------------------------

    fn toggle_hidden(&mut self, id: String, visible: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.update_config(window, cx, |c| {
            c.hidden_browsers.retain(|h| h != &id);
            if !visible {
                c.hidden_browsers.push(id);
            }
        });
    }

    fn remove_custom(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.update_config(window, cx, |c| c.custom_browsers.retain(|b| b.id() != id)) {
            self.reload(window, cx);
        }
    }

    fn open_custom_browser_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Portable Firefox"));
        let exe = cx.new(|cx| InputState::new(window, cx).placeholder(r"C:\Tools\browser\browser.exe"));
        let args = cx.new(|cx| InputState::new(window, cx).placeholder("{url}  (optional, e.g. --new-window {url})"));
        let view = cx.entity();
        let focus_once = Cell::new(true);
        let submit: Callback = {
            let (name, exe, args) = (name.clone(), exe.clone(), args.clone());
            Rc::new(move |window, cx| {
                let browser = CustomBrowser {
                    name: name.read(cx).value().trim().to_string(),
                    exe: exe.read(cx).value().trim().trim_matches('"').into(),
                    args: args.read(cx).value().trim().to_string(),
                };
                let error = if browser.name.is_empty() {
                    Some("Give the browser a name.".to_string())
                } else if !browser.exe.is_file() {
                    Some(format!("{} doesn't exist.", browser.exe.display()))
                } else {
                    None
                };
                if let Some(e) = error {
                    window.push_notification(Notification::warning(e), cx);
                    return;
                }
                let saved = view.update(cx, |this, cx| {
                    if this.browsers.iter().any(|b| b.id == browser.id()) {
                        window.push_notification(Notification::warning("A browser with that name already exists."), cx);
                        return false;
                    }
                    let ok = this.update_config(window, cx, |c| c.custom_browsers.push(browser));
                    if ok {
                        this.reload(window, cx);
                    }
                    ok
                });
                if saved {
                    window.close_dialog(cx);
                }
            })
        };
        window.open_dialog(cx, move |dialog, window, cx| {
            if focus_once.replace(false) {
                name.focus_handle(cx).focus(window, cx);
            }
            let p = *cx.global::<Palette>();
            let submit = submit.clone();
            let exe_for_browse = exe.clone();
            dialog
                .title(dialog_title("Add a browser"))
                .w(px(520.))
                .child(
                    v_flex()
                        .gap_2()
                        .child(div().text_sm().child("Name"))
                        .child(Input::new(&name))
                        .child(div().text_sm().pt_2().child("Program"))
                        .child(
                            h_flex().gap_2().child(div().flex_1().child(Input::new(&exe))).child(
                                Button::new("browse").label("Browse…").on_click(move |_, window, cx| {
                                    let exe = exe_for_browse.clone();
                                    let paths = cx.prompt_for_paths(PathPromptOptions {
                                        files: true,
                                        directories: false,
                                        multiple: false,
                                        prompt: Some("Choose browser".into()),
                                    });
                                    window
                                        .spawn(cx, async move |cx| {
                                            let picked = paths.await.ok().and_then(|r| r.ok()).flatten();
                                            if let Some(path) = picked.and_then(|v| v.into_iter().next()) {
                                                let _ = cx.update(|window, cx| {
                                                    exe.update(cx, |s, cx| s.set_value(path.display().to_string(), window, cx))
                                                });
                                            }
                                        })
                                        .detach();
                                }),
                            ),
                        )
                        .child(div().text_sm().pt_2().child("Arguments"))
                        .child(Input::new(&args))
                        .child(
                            div()
                                .text_xs()
                                .text_color(p.text_secondary)
                                .child("{url} is replaced by the link; it's added at the end if you leave it out."),
                        ),
                )
                .footer(dialog_footer("Add", false, move |window, cx| submit(window, cx)))
        });
    }

    pub(super) fn render_browsers(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let cards = self.browsers.iter().map(|b| {
            let visible = !self.config.hidden_browsers.contains(&b.id);
            let mut details = b.exe.display().to_string();
            if !b.profiles.is_empty() {
                let names: Vec<&str> = b.profiles.iter().map(|p| p.name.as_str()).collect();
                details = format!("{} profile{}: {}", names.len(), if names.len() == 1 { "" } else { "s" }, names.join(", "));
            }
            let kind = match (&b.custom_args, b.family) {
                (Some(_), _) => "Custom",
                (None, Family::Chromium) => "Chromium",
                (None, Family::Firefox) => "Firefox",
                (None, Family::Other) => "Browser",
            };
            let id = b.id.clone();
            let remove_id = b.id.clone();
            card_with(
                p,
                browser_icon(&self.icons, &b.id, px(28.), p),
                h_flex().gap_2().items_center().child(b.name.clone()).child(chip(p, kind)),
                details,
            )
            .when(b.custom_args.is_some(), |d| {
                d.child(
                    Button::new(SharedString::from(format!("remove-{}", b.id)))
                        .ghost()
                        .child(icon(glyph::DELETE))
                        .tooltip("Remove")
                        .on_click(cx.listener(move |this, _, window, cx| this.remove_custom(remove_id.clone(), window, cx))),
                )
            })
            .child(
                Switch::new(SharedString::from(format!("show-{}", b.id)))
                    .checked(visible)
                    .tooltip(if visible { "Shown in lists" } else { "Hidden from lists" })
                    .on_click(cx.listener(move |this, checked: &bool, window, cx| {
                        this.toggle_hidden(id.clone(), *checked, window, cx)
                    })),
            )
        });
        let empty = self.browsers.is_empty();

        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(page_title("Browsers"))
                    .child(
                        Button::new("rescan")
                            .child(icon_label(glyph::REFRESH, "Rescan"))
                            .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx))),
                    )
                    .child(
                        Button::new("add-browser")
                            .primary()
                            .child(icon_label(glyph::ADD, "Add browser"))
                            .on_click(cx.listener(|this, _, window, cx| this.open_custom_browser_dialog(window, cx))),
                    ),
            )
            .child(
                div()
                    .pt_2()
                    .pb_2()
                    .text_sm()
                    .text_color(p.text_secondary)
                    .child("Found from Windows' list of installed browsers. Turn one off to hide it from the picker and target lists."),
            )
            .children(cards)
            .when(empty, |d| d.child(card(p, glyph::INFO, "No browsers found", "Add one by hand with “Add browser”")))
    }

    // ---- history -------------------------------------------------------

    pub(super) fn render_history(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let rows = self.history.iter().enumerate().map(|(i, entry)| {
            let host = rules::host_of(&entry.url).map(|h| h.trim_start_matches("www.").to_string());
            let destination = match &entry.target {
                Some(t) => format!("→ {}", self.label(t)),
                None => "Dismissed".into(),
            };
            let rule_name = entry
                .rule
                .as_ref()
                .and_then(|id| self.config.rules.iter().find(|r| &r.id == id))
                .map(|r| format!(" · rule “{}”", r.name))
                .unwrap_or_default();
            let source = entry.source.clone().filter(|s| !source::is_broker(s));
            let (view, host_for_menu, source_for_menu) = (view.clone(), host.clone(), source.clone());
            let target = entry.target.clone();
            card_with(
                p,
                match &entry.target {
                    Some(t) => browser_icon(&self.icons, &t.browser, px(20.), p),
                    None => icon(glyph::LINK).text_size(px(20.)).w(px(20.)).into_any_element(),
                },
                div().truncate().child(entry.url.clone()),
                format!("{} · {destination}{rule_name}", ago(entry.time)),
            )
            .when_some(source.clone(), |d, s| d.child(chip(p, format!("from {s}"))))
            .when(host.is_some() || source.is_some(), |d| {
                d.child(
                    Button::new(SharedString::from(format!("history-rule-{i}")))
                        .child(icon_label(glyph::ADD, "Rule"))
                        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                            let mut menu = menu;
                            let make = |matchers: Vec<Matcher>, view: Entity<SettingsView>, target: Option<_>| {
                                move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                                    let matchers = matchers.clone();
                                    let target = target.clone();
                                    view.update(cx, |this, cx| {
                                        this.page = Page::Rules;
                                        this.open_editor(None, matchers, target, window, cx)
                                    })
                                }
                            };
                            if let Some(h) = &host_for_menu {
                                menu = menu.item(
                                    PopupMenuItem::new(format!("Links to {h}"))
                                        .on_click(make(vec![Matcher::Domain(h.clone())], view.clone(), target.clone())),
                                );
                            }
                            if let Some(s) = &source_for_menu {
                                menu = menu.item(
                                    PopupMenuItem::new(format!("Links opened from {s}"))
                                        .on_click(make(vec![Matcher::SourceApp(s.clone())], view.clone(), target.clone())),
                                );
                            }
                            if let (Some(h), Some(s)) = (&host_for_menu, &source_for_menu) {
                                menu = menu.item(PopupMenuItem::new(format!("Links to {h} opened from {s}")).on_click(make(
                                    vec![Matcher::Domain(h.clone()), Matcher::SourceApp(s.clone())],
                                    view.clone(),
                                    target.clone(),
                                )));
                            }
                            menu
                        }),
                )
            })
        });
        let empty = self.history.is_empty();

        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(page_title("Recent links"))
                    .child(
                        Button::new("refresh-history")
                            .child(icon_label(glyph::REFRESH, "Refresh"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.history = history::load(&history::default_path());
                                cx.notify();
                            })),
                    )
                    .child(Button::new("clear-history").label("Clear").disabled(empty).on_click(cx.listener(
                        |this, _, window, cx| {
                            if let Err(e) = history::clear(&history::default_path()) {
                                window.push_notification(Notification::error(format!("{e:#}")), cx);
                            }
                            this.history.clear();
                            cx.notify();
                        },
                    ))),
            )
            .child(
                div()
                    .pt_2()
                    .pb_2()
                    .text_sm()
                    .text_color(p.text_secondary)
                    .child("The last links Crossroads handled, with the app they came from. Turn one into a rule with “Rule”."),
            )
            .children(rows)
            .when(empty, |d| {
                d.child(card(p, glyph::HISTORY, "Nothing yet", "Links you open will show up here once Crossroads is your default browser"))
            })
    }
}

use gpui_kit::component::Disableable as _;

//! Rules page (list + live tester) and the rule editor sub-page.

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::super::fluent::{Palette, glyph};
use super::super::widgets::{
    browser_icon, card, card_with, chip, dialog_footer, dialog_title, error_line, icon, icon_label, page_title, section,
};
use super::SettingsView;
use crate::model::browsers;
use crate::model::config::{Fallback, Matcher, MatcherKind, Rule, Target};
use crate::model::rules::{self, CompiledRule, Decision, LinkContext};

struct ConditionRow {
    /// Stable element id across re-renders.
    id: usize,
    kind: MatcherKind,
    input: Entity<InputState>,
}

/// A rule being created or edited.
pub struct RuleEditor {
    /// Id of the rule being edited; `None` for a new rule.
    existing: Option<String>,
    name: Entity<InputState>,
    rows: Vec<ConditionRow>,
    next_row: usize,
    target: Option<Target>,
    enabled: bool,
    _subscriptions: Vec<Subscription>,
}

impl RuleEditor {
    /// The rule as currently entered, or the first problem with it.
    fn build(&self, cx: &App) -> Result<Rule, String> {
        let matchers: Vec<Matcher> = self
            .rows
            .iter()
            .map(|r| r.kind.make(r.input.read(cx).value().trim().to_string()))
            .collect();
        if matchers.is_empty() {
            return Err("Add at least one condition".into());
        }
        for m in &matchers {
            rules::validate(m)?;
        }
        let target = self.target.clone().ok_or("Choose where these links open")?;
        let mut name = self.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            name = matchers.iter().find(|m| !m.is_source()).unwrap_or(&matchers[0]).value().to_string();
        }
        Ok(Rule { id: self.existing.clone().unwrap_or_default(), name, enabled: self.enabled, matchers, target })
    }
}

/// Short description of a rule's conditions, for its card.
fn summary(rule: &Rule) -> Vec<String> {
    rule.matchers
        .iter()
        .map(|m| match m {
            Matcher::Domain(d) => rules::normalize_domain(d),
            Matcher::Wildcard(w) => w.clone(),
            Matcher::Regex(r) => format!("/{r}/"),
            Matcher::SourceApp(a) => format!("from {}", rules::normalize_app(a)),
        })
        .collect()
}

impl SettingsView {
    // ---- editor --------------------------------------------------------

    /// Opens the editor for rule `existing`, or for a new rule prefilled with
    /// `matchers` / `target`.
    pub(super) fn open_editor(
        &mut self,
        existing: Option<String>,
        matchers: Vec<Matcher>,
        target: Option<Target>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rule = existing.as_ref().and_then(|id| self.config.rules.iter().find(|r| &r.id == id)).cloned();
        let (name, matchers, target, enabled) = match &rule {
            Some(r) => (r.name.clone(), r.matchers.clone(), Some(r.target.clone()), r.enabled),
            None => {
                let name = matchers.first().map(|m| rules::normalize_domain(m.value())).unwrap_or_default();
                let matchers = if matchers.is_empty() { vec![Matcher::Domain(String::new())] } else { matchers };
                let target = target.or_else(|| self.config.default_target.clone()).map(|t| Target { private: false, ..t });
                (if rule.is_none() && matchers.iter().all(|m| m.value().is_empty()) { String::new() } else { name }, matchers, target, true)
            }
        };
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Work tools").default_value(name));
        let mut editor = RuleEditor {
            existing,
            name: name_input,
            rows: Vec::new(),
            next_row: 0,
            target,
            enabled,
            _subscriptions: Vec::new(),
        };
        for m in matchers {
            self.push_row(&mut editor, m.kind(), m.value().to_string(), window, cx);
        }
        if let Some(first) = editor.rows.first() {
            first.input.focus_handle(cx).focus(window, cx);
        }
        self.editor = Some(editor);
        cx.notify();
    }

    fn push_row(&self, editor: &mut RuleEditor, kind: MatcherKind, value: String, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(kind.placeholder()).default_value(value));
        editor._subscriptions.push(cx.subscribe_in(&input, window, |_, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                cx.notify();
            }
        }));
        editor.rows.push(ConditionRow { id: editor.next_row, kind, input });
        editor.next_row += 1;
    }

    fn add_condition(&mut self, kind: MatcherKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut editor) = self.editor.take() else { return };
        self.push_row(&mut editor, kind, String::new(), window, cx);
        if let Some(row) = editor.rows.last() {
            row.input.focus_handle(cx).focus(window, cx);
        }
        self.editor = Some(editor);
        cx.notify();
    }

    fn save_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else { return };
        let mut rule = match editor.build(cx) {
            Ok(rule) => rule,
            Err(_) => return,
        };
        let existing = editor.existing.clone();
        let saved = self.update_config(window, cx, |c| match existing.and_then(|id| c.rules.iter().position(|r| r.id == id)) {
            Some(i) => c.rules[i] = rule,
            None => {
                rule.id = c.new_rule_id();
                c.rules.push(rule);
            }
        });
        if saved {
            self.editor = None;
            cx.notify();
        }
    }

    pub(super) fn render_editor(&self, p: &Palette, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = self.editor.as_ref().expect("editor open");
        let result = editor.build(cx);
        let view = cx.entity();

        let rows = editor.rows.iter().enumerate().map(|(i, row)| {
            let value = row.input.read(cx).value().trim().to_string();
            let error = if value.is_empty() { None } else { rules::validate(&row.kind.make(value)).err() };
            let row_id = row.id;
            let view = view.clone();
            let kind_button = Button::new(SharedString::from(format!("kind-{row_id}")))
                .w(px(170.))
                .child(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .items_center()
                        .child(div().flex_1().child(row.kind.label()))
                        .child(icon(glyph::DOWN).text_size(px(10.)).text_color(p.text_secondary)),
                )
                .dropdown_menu(move |menu, _, _| {
                    MatcherKind::ALL.into_iter().fold(menu, |menu, kind| {
                        let view = view.clone();
                        menu.item(PopupMenuItem::new(kind.label()).on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                if let Some(row) = this.editor.as_mut().and_then(|e| e.rows.iter_mut().find(|r| r.id == row_id)) {
                                    row.kind = kind;
                                }
                                cx.notify();
                            })
                        }))
                    })
                });
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().w(px(36.)).text_xs().text_color(p.text_tertiary).child(if i == 0 { "" } else { "or" }))
                        .child(kind_button)
                        .child(div().flex_1().child(Input::new(&row.input)))
                        .child(
                            Button::new(SharedString::from(format!("remove-row-{row_id}")))
                                .ghost()
                                .child(icon(glyph::DELETE))
                                .tooltip("Remove condition")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(e) = this.editor.as_mut() {
                                        e.rows.retain(|r| r.id != row_id);
                                    }
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .pl(px(46.))
                        .text_xs()
                        .text_color(p.text_tertiary)
                        .child(format!("e.g. {}", row.kind.placeholder())),
                )
                .when_some(error, |d, e| d.child(div().pl(px(46.)).child(error_line(p, e))))
        }).collect::<Vec<_>>();

        let view_add = cx.entity();
        let add_condition = Button::new("add-condition").child(icon_label(glyph::ADD, "Add condition")).dropdown_menu(move |menu, _, _| {
            MatcherKind::ALL.into_iter().fold(menu, |menu, kind| {
                let view = view_add.clone();
                menu.item(
                    PopupMenuItem::new(kind.label())
                        .on_click(move |_, window, cx| view.update(cx, |this, cx| this.add_condition(kind, window, cx))),
                )
            })
        });

        let supports_private = editor
            .target
            .as_ref()
            .and_then(|t| browsers::find(&self.browsers, &t.browser))
            .is_some_and(|b| b.supports_private());
        let private = editor.target.as_ref().is_some_and(|t| t.private);

        let target_card = card(p, glyph::GLOBE, "Open in", "Browser and profile for links this rule matches")
            .child(self.target_picker(
                "rule-target",
                editor.target.as_ref(),
                p,
                |this, target, _, cx| {
                    if let Some(e) = this.editor.as_mut() {
                        let private = e.target.as_ref().is_some_and(|t| t.private);
                        e.target = Some(Target { private, ..target });
                    }
                    cx.notify();
                },
                cx,
            ));

        let private_card = card(p, glyph::LOCK, "Private window", "Open these links in an InPrivate / incognito / private window")
            .child(Switch::new("rule-private").checked(private && supports_private).disabled(!supports_private).on_click(cx.listener(
                |this, checked: &bool, _, cx| {
                    if let Some(t) = this.editor.as_mut().and_then(|e| e.target.as_mut()) {
                        t.private = *checked;
                    }
                    cx.notify();
                },
            )));

        let is_new = editor.existing.is_none();
        let error = result.as_ref().err().cloned();
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("back")
                            .ghost()
                            .child(icon(glyph::BACK))
                            .tooltip("Back to rules")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.editor = None;
                                cx.notify();
                            })),
                    )
                    .child(page_title(if is_new { "New rule" } else { "Edit rule" })),
            )
            .child(section("Name"))
            .child(div().max_w(px(420.)).child(Input::new(&editor.name)))
            .child(section("Conditions"))
            .child(
                div()
                    .pb_2()
                    .text_sm()
                    .text_color(p.text_secondary)
                    .child("Matches when any link condition matches. With “Opened from app” conditions, the link must also come from one of those apps."),
            )
            .child(
                v_flex()
                    .p_4()
                    .gap_3()
                    .rounded(px(8.))
                    .bg(p.card)
                    .border_1()
                    .border_color(p.stroke)
                    .children(rows)
                    .child(h_flex().pl(px(46.)).child(add_condition)),
            )
            .child(section("Destination"))
            .child(target_card)
            .child(private_card)
            .child(
                h_flex()
                    .pt_6()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("save-rule")
                            .primary()
                            .child(icon_label(glyph::SAVE, if is_new { "Add rule" } else { "Save" }))
                            .disabled(error.is_some())
                            .on_click(cx.listener(|this, _, window, cx| this.save_editor(window, cx))),
                    )
                    .child(Button::new("cancel-rule").label("Cancel").on_click(cx.listener(|this, _, _, cx| {
                        this.editor = None;
                        cx.notify();
                    })))
                    .when_some(error, |d, e| d.child(div().pl_2().text_xs().text_color(p.text_secondary).child(e))),
            )
    }

    // ---- list ------------------------------------------------------------

    fn move_rule(&mut self, index: usize, up: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.update_config(window, cx, |c| {
            let other = if up { index.checked_sub(1) } else { Some(index + 1) };
            if let Some(other) = other.filter(|o| *o < c.rules.len()) {
                c.rules.swap(index, other);
            }
        });
    }

    fn open_delete_dialog(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rule) = self.config.rules.iter().find(|r| r.id == id).cloned() else { return };
        let view = cx.entity();
        window.open_dialog(cx, move |dialog, _, cx| {
            let p = *cx.global::<Palette>();
            let view = view.clone();
            let id = rule.id.clone();
            dialog
                .title(dialog_title(format!("Delete “{}”?", rule.name)))
                .w(px(420.))
                .child(div().text_color(p.text_secondary).child("Links it matched will fall through to the next rule or the default."))
                .footer(dialog_footer("Delete", true, move |window, cx| {
                    let id = id.clone();
                    view.update(cx, |this, cx| {
                        this.update_config(window, cx, |c| c.rules.retain(|r| r.id != id));
                    });
                    window.close_dialog(cx);
                }))
        });
    }

    fn render_tester(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let url = self.test_url.read(cx).value().trim().to_string();
        let source = self.test_source.read(cx).value().trim().to_string();
        let outcome: Option<(String, Option<String>)> = (!url.is_empty()).then(|| {
            let ctx = LinkContext {
                url: url.clone(),
                source_exe: (!source.is_empty()).then(|| rules::normalize_app(&source)),
                force_picker: false,
            };
            match rules::route(&self.config, &ctx, |t| browsers::is_available(&self.browsers, t)) {
                Decision::Open { target, rule: Some(id) } => {
                    let name = self.config.rules.iter().find(|r| r.id == id).map(|r| r.name.clone()).unwrap_or(id);
                    (format!("Rule “{name}” → {}", self.label(&target)), Some(target.browser))
                }
                Decision::Open { target, rule: None } => {
                    (format!("No rule matches → default: {}", self.label(&target)), Some(target.browser))
                }
                Decision::Ask => (
                    match self.config.fallback {
                        Fallback::Picker => "No rule matches → you'll be asked".to_string(),
                        Fallback::Default => "No rule matches and no default browser is set → you'll be asked".to_string(),
                    },
                    None,
                ),
            }
        });

        v_flex()
            .p_4()
            .gap_3()
            .rounded(px(8.))
            .bg(p.card)
            .border_1()
            .border_color(p.stroke)
            .child(
                h_flex()
                    .gap_4()
                    .items_center()
                    .child(icon(glyph::SEARCH).text_size(px(20.)).w(px(20.)))
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child("Try a link")
                            .child(div().text_xs().text_color(p.text_secondary).child("See which rule catches it and where it would open")),
                    ),
            )
            .child(
                h_flex()
                    .pl(px(36.))
                    .gap_2()
                    .child(div().flex_1().child(Input::new(&self.test_url)))
                    .child(div().w(px(220.)).child(Input::new(&self.test_source))),
            )
            .when_some(outcome, |d, (text, browser)| {
                d.child(
                    h_flex()
                        .pl(px(36.))
                        .gap_2()
                        .items_center()
                        .when_some(browser, |d, b| d.child(browser_icon(&self.icons, &b, px(16.), p)))
                        .child(div().font_weight(FontWeight::SEMIBOLD).text_color(p.accent_text).child(text)),
                )
            })
    }

    pub(super) fn render_rules(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.config.rules.len();
        let cards = self.config.rules.iter().enumerate().map(|(i, rule)| {
            let compiled = CompiledRule::new(rule);
            let error = compiled.error.clone();
            let available = browsers::is_available(&self.browsers, &rule.target);
            let chips = summary(rule).into_iter().map(|s| chip(p, s));
            let (toggle_id, edit_id, delete_id) = (rule.id.clone(), rule.id.clone(), rule.id.clone());
            let enabled = rule.enabled;
            card_with(
                p,
                div().w(px(20.)).text_color(p.text_tertiary).text_sm().child(format!("{}", i + 1)),
                h_flex()
                    .gap_2()
                    .items_center()
                    .when(!enabled, |d| d.text_color(p.text_tertiary))
                    .child(div().truncate().child(rule.name.clone())),
                v_flex()
                    .gap_1()
                    .pt_1()
                    .child(h_flex().flex_wrap().gap_1().children(chips))
                    .when_some(error, |d, e| d.child(error_line(p, e)))
                    .when(!available, |d| d.child(error_line(p, "Browser or profile not found; this rule is skipped"))),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .flex_shrink_0()
                    .child(browser_icon(&self.icons, &rule.target.browser, px(20.), p))
                    .child(div().max_w(px(200.)).truncate().text_sm().child(self.label(&rule.target))),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .child(
                        Button::new(SharedString::from(format!("up-{i}")))
                            .ghost()
                            .child(icon(glyph::UP).text_size(px(12.)))
                            .tooltip("Move up")
                            .disabled(i == 0)
                            .on_click(cx.listener(move |this, _, window, cx| this.move_rule(i, true, window, cx))),
                    )
                    .child(
                        Button::new(SharedString::from(format!("down-{i}")))
                            .ghost()
                            .child(icon(glyph::DOWN).text_size(px(12.)))
                            .tooltip("Move down")
                            .disabled(i + 1 == count)
                            .on_click(cx.listener(move |this, _, window, cx| this.move_rule(i, false, window, cx))),
                    )
                    .child(
                        Button::new(SharedString::from(format!("edit-{i}")))
                            .ghost()
                            .child(icon(glyph::EDIT))
                            .tooltip("Edit")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_editor(Some(edit_id.clone()), Vec::new(), None, window, cx)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("delete-{i}")))
                            .ghost()
                            .child(icon(glyph::DELETE))
                            .tooltip("Delete")
                            .on_click(cx.listener(move |this, _, window, cx| this.open_delete_dialog(delete_id.clone(), window, cx))),
                    ),
            )
            .child(
                Switch::new(SharedString::from(format!("enabled-{i}")))
                    .checked(enabled)
                    .tooltip(if enabled { "On" } else { "Off" })
                    .on_click(cx.listener(move |this, checked: &bool, window, cx| {
                        let id = toggle_id.clone();
                        let on = *checked;
                        this.update_config(window, cx, |c| {
                            if let Some(r) = c.rules.iter_mut().find(|r| r.id == id) {
                                r.enabled = on;
                            }
                        });
                    })),
            )
        }).collect::<Vec<_>>();

        v_flex()
            .gap_1()
            .child(
                h_flex().gap_2().items_center().child(page_title("Rules")).child(
                    Button::new("add-rule")
                        .primary()
                        .child(icon_label(glyph::ADD, "Add rule"))
                        .on_click(cx.listener(|this, _, window, cx| this.open_editor(None, Vec::new(), None, window, cx))),
                ),
            )
            .child(
                div()
                    .pt_2()
                    .pb_2()
                    .text_sm()
                    .text_color(p.text_secondary)
                    .child("Checked from top to bottom; the first rule that matches decides. Everything else follows “When no rule matches” in General."),
            )
            .child(self.render_tester(p, cx))
            .child(section(if count == 0 { "No rules yet".to_string() } else { format!("{count} rule{}", if count == 1 { "" } else { "s" }) }))
            .children(cards)
            .when(count == 0, |d| {
                d.child(card(
                    p,
                    glyph::FILTER,
                    "Send links to the right browser",
                    "For example: github.com → Chrome (Work), links from Slack → Edge, youtube.com → Firefox",
                ))
            })
    }
}

//! Full editor for a game's `OptiScaler.ini`.
//!
//! The document is re-read from disk every time the editor opens, because
//! OptiScaler rewrites the file from its own in-game overlay. Edits are held in
//! the parsed document and only written back when the user saves, which
//! preserves every comment and untouched line.

use std::collections::HashMap;
use std::path::PathBuf;

use gpui::{
    App, AppContext, ClickEvent, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, Styled, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, Disableable, IconName, IndexPath, Selectable, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    select::{Select, SelectState},
    v_flex,
};
use opti_core::GameId;
use opti_core::optiscaler::ini::{IniDocument, KeyInfo, ValueType};

use crate::app_state::AppState;

/// A control bound to one ini key.
enum Field {
    /// Tri-state toggles and enumerations.
    Choice(Entity<SelectState<Vec<SharedString>>>),
    /// Numbers and free text.
    Text(Entity<InputState>),
}

pub struct ConfigEditor {
    state: Entity<AppState>,
    game_id: GameId,
    path: PathBuf,
    doc: Option<IniDocument>,
    load_error: Option<String>,
    save_error: Option<String>,
    saved: bool,
    sections: Vec<String>,
    active_section: String,
    /// Controls for the active section only, keyed by ini key.
    fields: HashMap<String, Field>,
    /// Set when the controls need rebuilding on the next render, which is
    /// where a `Window` is available to construct them with.
    needs_fields: bool,
}

impl ConfigEditor {
    pub fn new(state: Entity<AppState>, game_id: GameId, cx: &mut Context<Self>) -> Self {
        let path = state
            .read(cx)
            .status_for(&game_id)
            .map(|status| status.target_dir.join("OptiScaler.ini"))
            .unwrap_or_default();

        let mut editor = ConfigEditor {
            state,
            game_id,
            path,
            doc: None,
            load_error: None,
            save_error: None,
            saved: false,
            sections: Vec::new(),
            active_section: String::new(),
            fields: HashMap::new(),
            needs_fields: false,
        };
        editor.reload(cx);
        editor
    }

    pub fn view(state: Entity<AppState>, game_id: GameId, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(state, game_id, cx))
    }

    /// Re-reads the ini from disk, discarding unsaved edits.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.saved = false;
        self.save_error = None;

        match std::fs::read_to_string(&self.path) {
            Ok(source) => {
                let doc = IniDocument::parse(&source);
                self.sections = doc.section_names();
                self.active_section = self
                    .sections
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "General".to_string());
                self.doc = Some(doc);
                self.load_error = None;
                self.needs_fields = true;
            }
            Err(err) => {
                self.doc = None;
                self.fields.clear();
                self.needs_fields = false;
                self.load_error = Some(format!("Could not read {}: {err}", self.path.display()));
            }
        }
        cx.notify();
    }

    fn keys_in_active_section(&self) -> Vec<KeyInfo> {
        let Some(doc) = &self.doc else {
            return Vec::new();
        };
        doc.keys()
            .into_iter()
            .filter(|key| key.section == self.active_section)
            .collect()
    }

    /// Creates the controls for the active section. Only one section's worth of
    /// widgets exists at a time, which keeps ~300 keys affordable.
    fn build_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fields.clear();

        for info in self.keys_in_active_section() {
            let field = match info.value_type {
                ValueType::Bool | ValueType::Enum(_) => {
                    let choices: Vec<SharedString> =
                        info.choices().into_iter().map(SharedString::from).collect();
                    // Values OptiScaler wrote that are not in our inferred list
                    // still need to show, so fall back to the first entry.
                    let selected = choices
                        .iter()
                        .position(|choice| choice.eq_ignore_ascii_case(&info.value))
                        .unwrap_or(0);
                    let state = cx.new(|cx| {
                        SelectState::new(
                            choices,
                            Some(IndexPath::default().row(selected)),
                            window,
                            cx,
                        )
                    });
                    Field::Choice(state)
                }
                _ => {
                    let value = info.value.clone();
                    let state = cx.new(|cx| {
                        InputState::new(window, cx)
                            .placeholder("auto")
                            .default_value(value)
                    });
                    Field::Text(state)
                }
            };
            self.fields.insert(info.key.clone(), field);
        }
    }

    /// Copies the current control values into the parsed document, so edits
    /// survive switching sections.
    fn apply_fields(&mut self, cx: &App) {
        let Some(doc) = &mut self.doc else { return };
        let section = self.active_section.clone();

        for (key, field) in &self.fields {
            let value = match field {
                Field::Choice(state) => state.read(cx).selected_value().map(|v| v.to_string()),
                Field::Text(state) => Some(state.read(cx).value().to_string()),
            };

            if let Some(value) = value {
                let value = value.trim();
                // An emptied text field means "back to the default".
                let value = if value.is_empty() { "auto" } else { value };
                if doc.get(&section, key) != Some(value) {
                    doc.set(&section, key, value);
                }
            }
        }
    }

    fn select_section(&mut self, section: String, cx: &mut Context<Self>) {
        if section == self.active_section {
            return;
        }
        self.apply_fields(cx);
        self.active_section = section;
        self.saved = false;
        self.needs_fields = true;
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        self.apply_fields(cx);
        let Some(doc) = &self.doc else { return };

        match std::fs::write(&self.path, doc.to_source()) {
            Ok(()) => {
                log::info!("saved {}", self.path.display());
                self.saved = true;
                self.save_error = None;
            }
            Err(err) => {
                self.save_error = Some(format!("Could not save: {err}"));
                self.saved = false;
            }
        }
        cx.notify();
    }

    fn render_field(&self, info: &KeyInfo, cx: &Context<Self>) -> impl IntoElement {
        let control = match self.fields.get(&info.key) {
            Some(Field::Choice(state)) => Select::new(state).small().w(px(220.)).into_any_element(),
            Some(Field::Text(state)) => Input::new(state).small().w(px(220.)).into_any_element(),
            None => div().into_any_element(),
        };

        v_flex()
            .w_full()
            .gap_1()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .w_full()
                    .gap_4()
                    .items_start()
                    .justify_between()
                    .child(
                        v_flex()
                            .gap_0p5()
                            .flex_1()
                            .overflow_hidden()
                            .child(div().text_sm().child(info.key.clone()))
                            .children(info.help.iter().map(|line| {
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(line.clone())
                            })),
                    )
                    .child(div().flex_shrink_0().child(control)),
            )
    }
}

impl Render for ConfigEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.needs_fields {
            self.build_fields(window, cx);
            self.needs_fields = false;
        }

        let title = self
            .state
            .read(cx)
            .game(&self.game_id)
            .map(|game| game.title.clone())
            .unwrap_or_default();

        if let Some(error) = &self.load_error {
            return v_flex()
                .gap_2()
                .child(div().text_lg().child(format!("{title} — configuration")))
                .child(
                    div()
                        .p_2()
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().danger.opacity(0.15))
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error.clone()),
                )
                .into_any_element();
        }

        let keys = self.keys_in_active_section();
        let sections = self.sections.clone();
        let active = self.active_section.clone();

        v_flex()
            .size_full()
            .gap_3()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_lg().child(format!("{title} — configuration")))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .when(self.saved, |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().success)
                                        .child("Saved"),
                                )
                            })
                            .child(
                                Button::new("reload")
                                    .small()
                                    .outline()
                                    .icon(IconName::Undo2)
                                    .label("Reload")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.reload(cx);
                                    })),
                            )
                            .child(
                                Button::new("save")
                                    .small()
                                    .primary()
                                    .label("Save")
                                    .disabled(self.doc.is_none())
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.save(cx);
                                    })),
                            ),
                    ),
            )
            .when_some(self.save_error.clone(), |this, message| {
                this.child(
                    div()
                        .p_2()
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().danger.opacity(0.15))
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(message),
                )
            })
            .child(
                h_flex()
                    .size_full()
                    .flex_1()
                    .gap_4()
                    .items_start()
                    // Section list.
                    .child(
                        v_flex()
                            .id("ini-sections")
                            .w(px(180.))
                            .flex_shrink_0()
                            .h_full()
                            .gap_0p5()
                            .children(sections.into_iter().map(|section| {
                                let is_active = section == active;
                                let for_click = section.clone();
                                Button::new(SharedString::from(format!("sec-{section}")))
                                    .small()
                                    .ghost()
                                    .selected(is_active)
                                    .label(section)
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.select_section(for_click.clone(), cx);
                                    }))
                            }))
                            .overflow_y_scrollbar(),
                    )
                    // Fields for the active section.
                    .child(
                        v_flex()
                            .id("ini-fields")
                            .flex_1()
                            .h_full()
                            .w_full()
                            .overflow_hidden()
                            .pr_2()
                            .children(keys.iter().map(|info| self.render_field(info, cx)))
                            .overflow_y_scrollbar(),
                    ),
            )
            .into_any_element()
    }
}

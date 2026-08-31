use gpui::{
    App, AppContext, ClickEvent, Context, Entity, IntoElement, ParentElement, Render, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
    divider::Divider,
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex,
};

use crate::app_state::AppState;

/// Application settings: the SteamGridDB key used for cover art on non-Steam
/// games, plus the on-disk locations the app uses.
pub struct SettingsView {
    state: Entity<AppState>,
    key_input: Entity<InputState>,
    saved: bool,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let existing = state
            .read(cx)
            .settings
            .steamgriddb_key()
            .unwrap_or_default()
            .to_string();

        let key_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Paste your SteamGridDB API key")
                .default_value(existing)
        });

        // Any edit invalidates the "Saved" note.
        let subscription = cx.subscribe(&key_input, |this: &mut Self, _, _: &InputEvent, cx| {
            this.saved = false;
            cx.notify();
        });

        SettingsView {
            state,
            key_input,
            saved: false,
            _subscriptions: vec![subscription],
        }
    }

    pub fn view(state: Entity<AppState>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(state, window, cx))
    }

    fn save_key(&mut self, cx: &mut Context<Self>) {
        let value = self.key_input.read(cx).value().trim().to_string();
        let key = (!value.is_empty()).then_some(value);

        self.state
            .update(cx, |state, cx| state.set_steamgriddb_key(key, cx));
        self.saved = true;
        cx.notify();
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cache_dir = opti_core::paths::cache_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|err| format!("unavailable: {err}"));
        let config_dir = opti_core::paths::config_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|err| format!("unavailable: {err}"));
        let game_count = self.state.read(cx).games.len();

        v_flex()
            .size_full()
            .gap_4()
            .child(div().text_lg().child("Settings"))
            .child(Divider::horizontal())
            .child(
                v_flex()
                    .gap_2()
                    .child(div().text_sm().child("SteamGridDB API key"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Steam games get their cover art from Steam directly. \
                                 Epic and Xbox games need a free SteamGridDB key; without \
                                 one they fall back to a generated placeholder.",
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Input::new(&self.key_input).w(px(360.)))
                            .child(
                                Button::new("save-key")
                                    .primary()
                                    .small()
                                    .label("Save")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.save_key(cx);
                                    })),
                            )
                            .when(self.saved, |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().success)
                                        .child("Saved — fetching artwork"),
                                )
                            }),
                    ),
            )
            .child(Divider::horizontal())
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child("Locations"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Settings and install records: {config_dir}")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Artwork and downloads: {cache_dir}")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{game_count} games detected")),
                    ),
            )
    }
}

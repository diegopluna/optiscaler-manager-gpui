use gpui::{
    App, AppContext, ClickEvent, Context, Entity, IntoElement, ParentElement, Render, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
    divider::Divider,
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex,
};

use std::path::PathBuf;

use crate::app_state::{AppState, UpdateState};

/// Application settings: the SteamGridDB key used for cover art on non-Steam
/// games, plus the on-disk locations the app uses.
pub struct SettingsView {
    state: Entity<AppState>,
    key_input: Entity<InputState>,
    game_input: Entity<InputState>,
    folder_input: Entity<InputState>,
    /// Feedback from the last add attempt, shown under the inputs.
    location_error: Option<String>,
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
        let game_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Path to one game's folder"));
        let folder_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(r"Library folder to scan, e.g. D:\Games")
        });

        // Any edit invalidates the "Saved" note.
        let subscription = cx.subscribe(&key_input, |this: &mut Self, _, _: &InputEvent, cx| {
            this.saved = false;
            cx.notify();
        });

        SettingsView {
            state,
            key_input,
            game_input,
            folder_input,
            location_error: None,
            saved: false,
            _subscriptions: vec![subscription],
        }
    }

    /// Reads a path out of `input`, hands it to `add`, and clears the field
    /// on success. Errors from the add show under the inputs.
    fn add_location(
        &mut self,
        input: &Entity<InputState>,
        add: impl FnOnce(&mut AppState, PathBuf, &mut gpui::Context<AppState>) -> Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let typed = input.read(cx).value().trim().to_string();
        if typed.is_empty() {
            return;
        }

        let result = self
            .state
            .update(cx, |state, cx| add(state, PathBuf::from(&typed), cx));

        match result {
            Ok(()) => {
                self.location_error = None;
                input.update(cx, |state, cx| state.set_value("", window, cx));
            }
            Err(message) => self.location_error = Some(message),
        }
        cx.notify();
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
            .child({
                let update = self.state.read(cx).update.clone();
                let busy = update.is_busy();
                v_flex()
                    .gap_2()
                    .child(div().text_sm().child("Updates"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "This is version {}.",
                                opti_core::update::CURRENT_VERSION
                            )),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new("check-updates")
                                    .small()
                                    .outline()
                                    .label(match &update {
                                        UpdateState::Checking => "Checking…",
                                        UpdateState::Downloading => "Updating…",
                                        _ => "Check for updates",
                                    })
                                    .disabled(busy)
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.state
                                            .update(cx, |state, cx| state.check_for_updates(cx));
                                    })),
                            )
                            .when_some(
                                match &update {
                                    UpdateState::Available(info) => Some(info.tag.clone()),
                                    _ => None,
                                },
                                |this, tag| {
                                    this.child(
                                        Button::new("apply-update")
                                            .small()
                                            .primary()
                                            .label(format!("Update to {tag}"))
                                            .disabled(busy)
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _, cx| {
                                                    this.state.update(cx, |state, cx| {
                                                        state.apply_update(cx)
                                                    });
                                                },
                                            )),
                                    )
                                },
                            )
                            .when_some(
                                match &update {
                                    UpdateState::UpToDate => {
                                        Some(("Up to date".to_string(), false))
                                    }
                                    UpdateState::RestartRequired => Some((
                                        "Updated — restart the app to finish".to_string(),
                                        false,
                                    )),
                                    UpdateState::Failed(err) => Some((err.clone(), true)),
                                    _ => None,
                                },
                                |this, (message, is_error)| {
                                    this.child(
                                        div()
                                            .text_xs()
                                            .text_color(if is_error {
                                                cx.theme().danger
                                            } else {
                                                cx.theme().success
                                            })
                                            .child(message),
                                    )
                                },
                            ),
                    )
            })
            .child(Divider::horizontal())
            .child({
                let manual_games = self.state.read(cx).settings.manual_games.clone();
                let scan_folders = self.state.read(cx).settings.scan_folders.clone();
                v_flex()
                    .gap_2()
                    .child(div().text_sm().child("Game locations"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Steam, Epic and Xbox are found automatically. Anything \
                                 else — GOG, EA, Ubisoft, DRM-free installs — can be added \
                                 here: single game folders, or library folders whose \
                                 subfolders are all games.",
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Input::new(&self.game_input).small().w(px(360.)))
                            .child(
                                Button::new("add-game")
                                    .small()
                                    .outline()
                                    .label("Add game")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        let input = this.game_input.clone();
                                        this.add_location(
                                            &input,
                                            |state, dir, cx| state.add_manual_game(dir, cx),
                                            window,
                                            cx,
                                        );
                                    })),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Input::new(&self.folder_input).small().w(px(360.)))
                            .child(
                                Button::new("add-folder")
                                    .small()
                                    .outline()
                                    .label("Add scan folder")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        let input = this.folder_input.clone();
                                        this.add_location(
                                            &input,
                                            |state, dir, cx| state.add_scan_folder(dir, cx),
                                            window,
                                            cx,
                                        );
                                    })),
                            ),
                    )
                    .when_some(self.location_error.clone(), |this, message| {
                        this.child(div().text_xs().text_color(cx.theme().danger).child(message))
                    })
                    .children(manual_games.iter().enumerate().map(|(ix, dir)| {
                        let for_remove = dir.clone();
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new(("rm-game", ix))
                                    .small()
                                    .ghost()
                                    .label("Remove")
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        let dir = for_remove.clone();
                                        this.state.update(cx, |state, cx| {
                                            state.remove_manual_game(&dir, cx)
                                        });
                                    })),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("Game: {}", dir.display())),
                            )
                    }))
                    .children(scan_folders.iter().enumerate().map(|(ix, dir)| {
                        let for_remove = dir.clone();
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new(("rm-folder", ix))
                                    .small()
                                    .ghost()
                                    .label("Remove")
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        let dir = for_remove.clone();
                                        this.state.update(cx, |state, cx| {
                                            state.remove_scan_folder(&dir, cx)
                                        });
                                    })),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("Scan folder: {}", dir.display())),
                            )
                    }))
            })
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
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(match &self.state.read(cx).gpu {
                                Some(gpu) => format!("GPU: {gpu}"),
                                None => "GPU: not detected".to_string(),
                            }),
                    ),
            )
    }
}

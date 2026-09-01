use gpui::{
    App, AppContext, ClickEvent, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
    v_flex,
};

use std::path::PathBuf;

use crate::app_state::{AppState, UpdateState};

/// Application settings: the SteamGridDB key used for cover art on non-Steam
/// games, plus the on-disk locations the app uses.
pub struct SettingsView {
    state: Entity<AppState>,
    key_input: Entity<InputState>,
    location_input: Entity<InputState>,
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
                .masked(true)
                .default_value(existing)
        });
        let location_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(r"Path to a game folder or a library to scan, e.g. D:\Games")
        });

        // Any edit invalidates the "Saved" note.
        let subscription = cx.subscribe(&key_input, |this: &mut Self, _, _: &InputEvent, cx| {
            this.saved = false;
            cx.notify();
        });

        SettingsView {
            state,
            key_input,
            location_input,
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
            .id("settings-page")
            .size_full()
            .gap(px(14.))
            .pr_2()
            .pb_4()
            .child(
                div()
                    .text_lg()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Settings"),
            )
            .child({
                let update = self.state.read(cx).update.clone();
                let busy = update.is_busy();
                crate::views::ui::section("Updates", cx)
                    .child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .justify_between()
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child(format!(
                                                "Version {}",
                                                opti_core::update::CURRENT_VERSION
                                            )),
                                    )
                                    .child(crate::views::ui::hint(
                                        "Updates install silently; the app restarts itself.",
                                        cx,
                                    )),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
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
                                    .child(
                                        Button::new("check-updates")
                                            .small()
                                            .outline()
                                            .label(match &update {
                                                UpdateState::Checking => "Checking…",
                                                UpdateState::Downloading => "Downloading…",
                                                UpdateState::Installing => "Restarting…",
                                                UpdateState::UpToDate => "Check again",
                                                _ => "Check for updates",
                                            })
                                            .disabled(busy)
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _, cx| {
                                                    this.state.update(cx, |state, cx| {
                                                        state.check_for_updates(cx)
                                                    });
                                                },
                                            )),
                                    ),
                            ),
                    )
                    .when_some(
                        match &update {
                            UpdateState::UpToDate => Some(("Up to date".to_string(), false)),
                            UpdateState::Installing => Some((
                                "Updating — the app will close and reopen itself".to_string(),
                                false,
                            )),
                            UpdateState::RestartRequired => {
                                Some(("Updated — restart the app to finish".to_string(), false))
                            }
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
                    )
            })
            .child({
                let manual_games = self.state.read(cx).settings.manual_games.clone();
                let scan_folders = self.state.read(cx).settings.scan_folders.clone();
                let games = self.state.read(cx).games.clone();
                crate::views::ui::section("Game locations", cx)
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
                            .child(Input::new(&self.location_input).small().flex_1())
                            .child(
                                Button::new("add-game")
                                    .small()
                                    .outline()
                                    .label("Add game")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        let input = this.location_input.clone();
                                        this.add_location(
                                            &input,
                                            |state, dir, cx| state.add_manual_game(dir, cx),
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                Button::new("add-folder")
                                    .small()
                                    .outline()
                                    .label("Add scan folder")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        let input = this.location_input.clone();
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
                    .children(scan_folders.iter().enumerate().map(|(ix, dir)| {
                        let count = games
                            .iter()
                            .filter(|game| {
                                game.store == opti_core::Store::Manual
                                    && game.install_dir.starts_with(dir)
                            })
                            .count();
                        let detail = match count {
                            1 => "scan folder · 1 game".to_string(),
                            n => format!("scan folder · {n} games"),
                        };
                        location_row(("rm-folder", ix), dir, detail, cx).child(
                            Button::new(("rm-folder-btn", ix))
                                .small()
                                .ghost()
                                .label("Remove")
                                .on_click({
                                    let for_remove = dir.clone();
                                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        let dir = for_remove.clone();
                                        this.state.update(cx, |state, cx| {
                                            state.remove_scan_folder(&dir, cx)
                                        });
                                    })
                                }),
                        )
                    }))
                    .children(manual_games.iter().enumerate().map(|(ix, dir)| {
                        location_row(("rm-game", ix), dir, "game".to_string(), cx).child(
                            Button::new(("rm-game-btn", ix))
                                .small()
                                .ghost()
                                .label("Remove")
                                .on_click({
                                    let for_remove = dir.clone();
                                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        let dir = for_remove.clone();
                                        this.state.update(cx, |state, cx| {
                                            state.remove_manual_game(&dir, cx)
                                        });
                                    })
                                }),
                        )
                    }))
            })
            .child(
                crate::views::ui::section("Artwork", cx)
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
            .child(
                crate::views::ui::section("About", cx)
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} · {game_count} games detected",
                                match &self.state.read(cx).gpu {
                                    Some(gpu) => format!("GPU: {gpu}"),
                                    None => "GPU: not detected".to_string(),
                                }
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(crate::theme::tokens::faint_text())
                            .child(format!("Settings and install records: {config_dir}")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(crate::theme::tokens::faint_text())
                            .child(format!("Artwork and downloads: {cache_dir}")),
                    ),
            )
            .overflow_y_scrollbar()
    }
}

/// One saved location as an inner card: folder icon, path, and a type chip.
/// The caller appends its Remove button so the listener stays with the view.
fn location_row(
    id: impl Into<gpui::ElementId>,
    dir: &std::path::Path,
    detail: String,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    h_flex()
        .id(id)
        .gap_2()
        .items_center()
        .py_2()
        .px_3()
        .rounded(px(10.))
        .border_1()
        .border_color(crate::theme::tokens::card_border())
        .bg(crate::theme::tokens::inner_bg())
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(Icon::new(IconName::Folder).small()),
        )
        .child(
            div()
                .text_xs()
                .flex_1()
                .overflow_hidden()
                .text_color(cx.theme().foreground)
                .child(dir.display().to_string()),
        )
        .child(crate::views::ui::chip(detail, cx))
}

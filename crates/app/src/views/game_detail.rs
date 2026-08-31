use gpui::{
    App, AppContext, ClickEvent, Context, Entity, IntoElement, ParentElement, Styled, Window, div,
    img, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, Disableable, IconName, Selectable, Sizable,
    badge::Badge,
    button::{Button, ButtonGroup, ButtonVariants},
    divider::Divider,
    h_flex,
    input::{Input, InputState},
    v_flex,
};
use opti_core::GameId;
use opti_core::optiscaler::{InstallStatus, PROXY_DLL_NAMES};

use crate::app_state::AppState;

/// Emitted when the user wants to edit this game's `OptiScaler.ini`.
pub struct OpenConfig(pub GameId);

impl gpui::EventEmitter<OpenConfig> for GameDetail {}

/// Per-game page: install status and the actions available for it.
pub struct GameDetail {
    state: Entity<AppState>,
    game_id: GameId,
    /// Editor for the install directory, built on the first render because
    /// creating an input needs a `Window`.
    dir_input: Option<Entity<InputState>>,
}

impl GameDetail {
    pub fn new(state: Entity<AppState>, game_id: GameId, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        GameDetail {
            state,
            game_id,
            dir_input: None,
        }
    }

    pub fn view(state: Entity<AppState>, game_id: GameId, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(state, game_id, cx))
    }

    pub fn game_id(&self) -> &GameId {
        &self.game_id
    }

    /// Points this game's install at whatever directory the user typed.
    fn apply_dir_override(&mut self, cx: &mut Context<Self>) {
        let Some(input) = &self.dir_input else { return };
        let typed = input.read(cx).value().trim().to_string();
        if typed.is_empty() {
            return;
        }

        let id = self.game_id.clone();
        let dir = std::path::PathBuf::from(typed);
        self.state
            .update(cx, |state, cx| state.set_target_dir(&id, dir, cx));
    }

    /// Reveals the install directory in the OS file manager.
    fn open_target_dir(&self, cx: &App) {
        let Some(status) = self.state.read(cx).status_for(&self.game_id) else {
            return;
        };
        let dir = status.target_dir.clone();

        let command = if cfg!(target_os = "windows") {
            "explorer"
        } else if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };

        if let Err(err) = std::process::Command::new(command).arg(&dir).spawn() {
            log::warn!("could not open {}: {err}", dir.display());
        }
    }
}

impl gpui::Render for GameDetail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Seed the directory editor once the scan has resolved a target for
        // this game. Done before borrowing state so the borrow does not
        // outlive creating the input.
        if self.dir_input.is_none() {
            let resolved = self
                .state
                .read(cx)
                .status_for(&self.game_id)
                .map(|status| status.target_dir.display().to_string())
                .filter(|dir| !dir.is_empty());

            if let Some(seed) = resolved {
                self.dir_input = Some(cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder("Directory the game's executable lives in")
                        .default_value(seed)
                }));
            }
        }

        let state = self.state.read(cx);
        let Some(game) = state.game(&self.game_id).cloned() else {
            return div()
                .child("This game is no longer installed.")
                .into_any_element();
        };

        let status = state.status_for(&self.game_id);
        let artwork = state.artwork_for(&self.game_id).map(|p| p.to_path_buf());
        let proxy_name = state.proxy_name_for(&self.game_id);
        let latest = state.latest_release.clone();
        let busy = status.and_then(|s| s.busy.clone());
        let error = status.and_then(|s| s.error.clone());
        let install = status.map(|s| s.install.clone());

        let (status_label, status_detail) = match &install {
            Some(InstallStatus::Managed(manifest)) => (
                format!("OptiScaler {}", manifest.release_tag),
                format!(
                    "{} files, loaded as {}",
                    manifest.files.len(),
                    manifest.proxy_name
                ),
            ),
            Some(InstallStatus::Unmanaged { proxy_name }) => (
                "Installed manually".to_string(),
                match proxy_name {
                    Some(name) => format!("Found {name}; this app did not install it"),
                    None => "OptiScaler.ini found without a proxy DLL".to_string(),
                },
            ),
            _ => (
                "Not installed".to_string(),
                "OptiScaler is not present in this game".to_string(),
            ),
        };

        let installed_tag = install
            .as_ref()
            .and_then(|i| i.version().map(str::to_string));
        let update_available = matches!(
            (&installed_tag, &latest),
            (Some(current), Some(release)) if current != &release.tag
        );
        let is_managed = matches!(install, Some(InstallStatus::Managed(_)));
        let can_install = latest.is_some() && busy.is_none();

        let id_for_install = self.game_id.clone();
        let id_for_uninstall = self.game_id.clone();
        let proxy_for_click = proxy_name.clone();
        let selected_proxy = PROXY_DLL_NAMES
            .iter()
            .position(|name| *name == proxy_name)
            .unwrap_or(0);

        v_flex()
            .size_full()
            .gap_4()
            .child(
                h_flex()
                    .gap_4()
                    .items_start()
                    .child(
                        div()
                            .w(px(140.))
                            .h(px(200.))
                            .flex_shrink_0()
                            .rounded(cx.theme().radius)
                            .overflow_hidden()
                            .border_1()
                            .border_color(cx.theme().border)
                            .when_some(artwork, |this, path| this.child(img(path).size_full())),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .flex_1()
                            .child(div().text_xl().child(game.title.clone()))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(game.store.label()),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .pt_2()
                                    .child(Badge::new().child(status_label.clone())),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(status_detail),
                            ),
                    ),
            )
            .child(Divider::horizontal())
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child("Install directory"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Detected automatically. Change it if OptiScaler belongs \
                                 next to a different executable.",
                            ),
                    )
                    .children(self.dir_input.as_ref().map(|input| {
                        h_flex()
                            .gap_2()
                            .items_center()
                            .pt_1()
                            .child(Input::new(input).small().flex_1())
                            .child(
                                Button::new("apply-dir")
                                    .small()
                                    .outline()
                                    .label("Apply")
                                    .disabled(is_managed)
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.apply_dir_override(cx);
                                    })),
                            )
                            .child(
                                Button::new("open-dir")
                                    .small()
                                    .outline()
                                    .icon(IconName::FolderOpen)
                                    .label("Open")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.open_target_dir(cx);
                                    })),
                            )
                    })),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child("Load OptiScaler as"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "The DLL name the game loads. dxgi.dll suits most games; \
                                 switch if the game already ships one.",
                            ),
                    )
                    .child(
                        ButtonGroup::new("proxy-name")
                            .outline()
                            .compact()
                            .disabled(is_managed || busy.is_some())
                            .children(PROXY_DLL_NAMES.iter().enumerate().map(|(ix, name)| {
                                Button::new(("proxy", ix))
                                    .label(*name)
                                    .selected(ix == selected_proxy)
                            }))
                            .on_click(cx.listener(|this, clicks: &Vec<usize>, _, cx| {
                                let Some(&ix) = clicks.first() else { return };
                                let Some(name) = PROXY_DLL_NAMES.get(ix) else {
                                    return;
                                };
                                let id = this.game_id.clone();
                                this.state
                                    .update(cx, |state, cx| state.set_proxy_name(&id, name, cx));
                            })),
                    ),
            )
            .when_some(error, |this, message| {
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
                    .gap_2()
                    .child(
                        Button::new("install")
                            .primary()
                            .disabled(!can_install || (is_managed && !update_available))
                            .label(match (&busy, is_managed, update_available) {
                                (Some(step), _, _) => step.clone(),
                                (None, true, true) => format!(
                                    "Update to {}",
                                    latest.as_ref().map(|r| r.tag.as_str()).unwrap_or_default()
                                ),
                                (None, true, false) => "Up to date".to_string(),
                                (None, false, _) => match &latest {
                                    Some(release) => format!("Install {}", release.tag),
                                    None => "Checking for releases…".to_string(),
                                },
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                let id = id_for_install.clone();
                                this.state.update(cx, |state, cx| state.install(&id, cx));
                            })),
                    )
                    .when(
                        install.as_ref().is_some_and(InstallStatus::is_installed),
                        |this| {
                            this.child(
                                Button::new("configure")
                                    .outline()
                                    .icon(IconName::Settings2)
                                    .label("Configure")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        cx.emit(OpenConfig(this.game_id.clone()));
                                    })),
                            )
                        },
                    )
                    .when(is_managed, |this| {
                        this.child(
                            Button::new("uninstall")
                                .danger()
                                .disabled(busy.is_some())
                                .label("Uninstall")
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    let id = id_for_uninstall.clone();
                                    this.state.update(cx, |state, cx| state.uninstall(&id, cx));
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "OptiScaler will be loaded as {proxy_for_click}. Do not use it in \
                         online games with anti-cheat."
                    )),
            )
            .into_any_element()
    }
}

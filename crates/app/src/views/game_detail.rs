use gpui::{
    App, AppContext, ClickEvent, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    SharedString, Styled, Window, div, img, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, IndexPath, Selectable, Sizable,
    badge::Badge,
    button::{Button, ButtonGroup, ButtonVariants},
    divider::Divider,
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
    v_flex,
};
use opti_core::GameId;
use opti_core::optiscaler::{InstallStatus, PROXY_DLL_NAMES};

use crate::app_state::AppState;

/// Per-game page: install status and the actions available for it.
pub struct GameDetail {
    state: Entity<AppState>,
    game_id: GameId,
    /// Editor for the install directory, built on the first render because
    /// creating an input needs a `Window`.
    dir_input: Option<Entity<InputState>>,
    /// Picker for which OptiScaler release to install.
    version_select: Option<Entity<SelectState<Vec<SharedString>>>>,
    /// Release tags in the same order as the picker's entries.
    version_tags: Vec<String>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl GameDetail {
    pub fn new(state: Entity<AppState>, game_id: GameId, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        GameDetail {
            state,
            game_id,
            dir_input: None,
            version_select: None,
            version_tags: Vec::new(),
            _subscriptions: Vec::new(),
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

        // Build the version picker once the release list has arrived.
        let release_tags: Vec<String> = self
            .state
            .read(cx)
            .releases
            .iter()
            .map(|release| release.tag.clone())
            .collect();

        if !release_tags.is_empty() && self.version_tags != release_tags {
            let labels: Vec<SharedString> = self
                .state
                .read(cx)
                .releases
                .iter()
                .map(|release| {
                    if release.prerelease {
                        SharedString::from(format!("{} (pre-release)", release.tag))
                    } else {
                        SharedString::from(release.tag.clone())
                    }
                })
                .collect();

            let current = self
                .state
                .read(cx)
                .release_for(&self.game_id)
                .map(|release| release.tag.clone());
            let selected = current
                .and_then(|tag| release_tags.iter().position(|t| t == &tag))
                .unwrap_or(0);

            let picker = cx.new(|cx| {
                SelectState::new(labels, Some(IndexPath::default().row(selected)), window, cx)
            });

            self._subscriptions = vec![cx.subscribe(
                &picker,
                |this, picker, _: &SelectEvent<Vec<SharedString>>, cx| {
                    let Some(ix) = picker.read(cx).selected_index(cx).map(|path| path.row) else {
                        return;
                    };
                    let Some(tag) = this.version_tags.get(ix).cloned() else {
                        return;
                    };
                    let id = this.game_id.clone();
                    this.state
                        .update(cx, |state, cx| state.select_release(&id, &tag, cx));
                },
            )];

            self.version_select = Some(picker);
            self.version_tags = release_tags;
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
        let selected = state.release_for(&self.game_id).cloned();
        let busy = status.and_then(|s| s.busy.clone());
        let error = status.and_then(|s| s.error.clone());
        let install = status.map(|s| s.install.clone());
        let anticheat = status.map(|s| s.anticheat.clone()).unwrap_or_default();
        let anticheat_names = status.map(|s| s.anticheat_names()).unwrap_or_default();
        let conflicts = status.and_then(|s| s.conflicts.clone());
        let optipatcher_supported = status.and_then(|s| s.optipatcher_supported.clone());
        let optipatcher_installed = status.is_some_and(|s| s.optipatcher_installed);
        let gpu = state.gpu.clone();
        let dlss_inputs = state.dlss_inputs_enabled(&self.game_id);

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
        // With a version picked, "install" means whatever differs from what is
        // on disk, so switching versions can move backwards as well as forwards.
        let differs_from_installed = matches!(
            (&installed_tag, &selected),
            (Some(current), Some(release)) if current != &release.tag
        );
        let is_managed = matches!(install, Some(InstallStatus::Managed(_)));
        let can_install = selected.is_some() && busy.is_none();
        let selected_tag = selected.as_ref().map(|r| r.tag.clone()).unwrap_or_default();
        let selected_notes: Vec<String> = selected
            .as_ref()
            .map(|release| opti_core::text::markdown_to_plain(&release.notes))
            .unwrap_or_default();
        let selected_published = selected
            .as_ref()
            .map(|r| r.published_at.chars().take(10).collect::<String>())
            .unwrap_or_default();

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
            .when_some(gpu, |this, gpu| {
                let needs_spoofing = gpu.vendor.needs_spoofing();
                this.child(
                    v_flex()
                        .gap_1()
                        .child(div().text_sm().child(format!("GPU: {gpu}")))
                        .map(|this| {
                            if needs_spoofing {
                                this.child(
                                    h_flex()
                                        .gap_2()
                                        .items_center()
                                        .child(
                                            Switch::new("dlss-inputs")
                                                .label("Use DLSS inputs (spoofs an Nvidia GPU)")
                                                .checked(dlss_inputs)
                                                .on_click(cx.listener(
                                                    |this, checked: &bool, _, cx| {
                                                        let id = this.game_id.clone();
                                                        let enabled = *checked;
                                                        this.state.update(cx, |state, cx| {
                                                            state.set_dlss_inputs(
                                                                &id, enabled, cx,
                                                            )
                                                        });
                                                    },
                                                )),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(
                                            "Lets games show their DLSS and DLSS frame \
                                             generation options so OptiScaler can take them \
                                             over. Turn off only if spoofing causes problems \
                                             in this game.",
                                        ),
                                )
                            } else {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(
                                            "Nvidia GPU — DLSS inputs work natively, no \
                                             spoofing needed.",
                                        ),
                                )
                            }
                        }),
                )
            })
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(div().text_sm().child("OptiScaler version"))
                            .children(
                                self.version_select
                                    .as_ref()
                                    .map(|state| Select::new(state).small().w(px(220.))),
                            )
                            .when(!selected_published.is_empty(), |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!("released {selected_published}")),
                                )
                            }),
                    )
                    .when(!selected_notes.is_empty(), |this| {
                        this.child(
                            v_flex()
                                .id("changelog")
                                .mt_1()
                                .p_2()
                                .gap_0p5()
                                .max_h(px(180.))
                                .w_full()
                                .overflow_hidden()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(cx.theme().border)
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!("What's new in {selected_tag}")),
                                )
                                .children(selected_notes.iter().map(|line| {
                                    // Blank lines keep paragraphs apart.
                                    if line.is_empty() {
                                        div().h(px(6.))
                                    } else {
                                        div().text_xs().child(line.clone())
                                    }
                                }))
                                .overflow_y_scrollbar(),
                        )
                    }),
            )
            .when(!anticheat.is_empty(), |this| {
                this.child(
                    v_flex()
                        .gap_1()
                        .p_2()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().danger)
                        .bg(cx.theme().danger.opacity(0.12))
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .text_color(cx.theme().danger)
                                .child(Icon::new(IconName::TriangleAlert))
                                .child(div().text_sm().child(format!(
                                    "{anticheat_names} detected — installing OptiScaler \
                                     here can get your account banned"
                                ))),
                        )
                        .children(anticheat.iter().map(|detection| {
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "{}: {}",
                                    detection.name,
                                    detection.evidence.display()
                                ))
                        })),
                )
            })
            .when(anticheat.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            "No anti-cheat files found in this game. That does not cover \
                             server-side systems such as VAC, so avoid OptiScaler in \
                             anything you play online.",
                        ),
                )
            })
            .when_some(optipatcher_supported, |this, supported_exe| {
                let can_add = is_managed && busy.is_none() && !optipatcher_installed;
                this.child(
                    v_flex()
                        .gap_1()
                        .p_2()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .justify_between()
                                .child(
                                    v_flex()
                                        .gap_0p5()
                                        .child(div().text_sm().child(if optipatcher_installed {
                                            "OptiPatcher installed".to_string()
                                        } else {
                                            "OptiPatcher supported".to_string()
                                        }))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(format!(
                                                    "This game ({supported_exe}) is on OptiPatcher's \
                                                     list: it unlocks DLSS and DLSS-FG inputs without \
                                                     GPU spoofing or its overhead."
                                                )),
                                        ),
                                )
                                .when(!optipatcher_installed, |this| {
                                    this.child(
                                        Button::new("add-optipatcher")
                                            .small()
                                            .outline()
                                            .label("Install OptiPatcher")
                                            .disabled(!can_add)
                                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                                let id = this.game_id.clone();
                                                this.state.update(cx, |state, cx| {
                                                    state.install_optipatcher(&id, cx)
                                                });
                                            })),
                                    )
                                }),
                        )
                        .when(!is_managed && !optipatcher_installed, |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Install OptiScaler first; OptiPatcher loads through it."),
                            )
                        }),
                )
            })
            .when_some(conflicts, |this, files| {
                this.child(
                    v_flex()
                        .gap_1()
                        .p_2()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().warning)
                        .bg(cx.theme().warning.opacity(0.12))
                        .child(div().text_sm().child(
                            "These files are already in the game and were not put \
                             there by this app — another mod like ReShade may own them:",
                        ))
                        .children(files.iter().map(|file| {
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("  {file}"))
                        }))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    "Continuing moves them into optiscaler-manager.backup \
                                     and puts them back on uninstall. The mod they belong \
                                     to will not work while OptiScaler is installed.",
                                ),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .pt_1()
                                .child(
                                    Button::new("confirm-conflicts")
                                        .small()
                                        .primary()
                                        .label("Back up and continue")
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            let id = this.game_id.clone();
                                            this.state.update(cx, |state, cx| {
                                                state.install_confirmed(&id, cx)
                                            });
                                        })),
                                )
                                .child(
                                    Button::new("cancel-conflicts")
                                        .small()
                                        .outline()
                                        .label("Cancel")
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            let id = this.game_id.clone();
                                            this.state.update(cx, |state, cx| {
                                                state.dismiss_conflicts(&id, cx)
                                            });
                                        })),
                                ),
                        ),
                )
            })
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
                            .disabled(!can_install || (is_managed && !differs_from_installed))
                            .label(match (&busy, is_managed, differs_from_installed) {
                                (Some(step), _, _) => step.clone(),
                                (None, true, true) => format!("Switch to {selected_tag}"),
                                (None, true, false) => format!("{selected_tag} installed"),
                                (None, false, _) => match selected.is_some() {
                                    true => format!("Install {selected_tag}"),
                                    false => "Checking for releases…".to_string(),
                                },
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                let id = id_for_install.clone();
                                this.state.update(cx, |state, cx| state.install(&id, cx));
                            })),
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
                        "OptiScaler will be loaded as {proxy_for_click}. Configure it from \
                         its own overlay in game — press Insert once the game is running."
                    )),
            )
            .into_any_element()
    }
}

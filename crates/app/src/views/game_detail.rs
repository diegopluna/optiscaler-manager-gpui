use gpui::{
    App, AppContext, ClickEvent, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    SharedString, Styled, Window, div, img, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, IndexPath, Selectable, Sizable,
    button::{Button, ButtonGroup, ButtonVariants},
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
        let upscaler_files = status.map(|s| s.upscalers.clone()).unwrap_or_default();
        let upscaler_techs = opti_core::upscalers::techs(&upscaler_files);
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
        let selected_notes: Vec<opti_core::text::NoteLine> = selected
            .as_ref()
            .map(|release| opti_core::text::markdown_note_lines(&release.notes))
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
            .id("game-detail")
            .size_full()
            .gap(px(14.))
            .pr_2()
            .pb_4()
            .child(
                h_flex()
                    .gap_5()
                    .items_start()
                    .child(
                        div()
                            .w(px(176.))
                            .h(px(254.))
                            .flex_shrink_0()
                            .rounded(px(12.))
                            .overflow_hidden()
                            .border_1()
                            .border_color(crate::theme::tokens::cover_border())
                            .shadow_lg()
                            .when_some(artwork, |this, path| this.child(img(path).size_full())),
                    )
                    .child(
                        v_flex()
                            .gap_1p5()
                            .flex_1()
                            // Match the cover's height so the action row can
                            // sit flush with its bottom edge, per the canvas.
                            .h(px(254.))
                            .pt_1()
                            .child(
                                div()
                                    .text_size(px(28.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(game.title.clone()),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .pt_1()
                                    .child(crate::views::ui::chip(game.store.label(), cx))
                                    .child({
                                        use crate::theme::tokens;
                                        let base = h_flex()
                                            .h(px(22.))
                                            .gap_1()
                                            .items_center()
                                            .px_2()
                                            .rounded(px(6.))
                                            .border_1()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::MEDIUM);
                                        if is_managed {
                                            base.bg(tokens::success_pill_bg())
                                                .border_color(tokens::success_pill_border())
                                                .text_color(tokens::success_pill_text())
                                                .child(
                                                    Icon::new(IconName::Check).xsmall(),
                                                )
                                                .child(status_label.clone())
                                        } else {
                                            base.bg(cx.theme().secondary)
                                                .border_color(cx.theme().border)
                                                .text_color(cx.theme().muted_foreground)
                                                .child(status_label.clone())
                                        }
                                    })
                                    .when(!upscaler_techs.is_empty(), |this| {
                                        this.child(crate::views::ui::chip(
                                            upscaler_techs
                                                .iter()
                                                .map(|tech| tech.label())
                                                .collect::<Vec<_>>()
                                                .join(" · "),
                                            cx,
                                        ))
                                    }),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(status_detail),
                            )
                            .child(div().flex_1())
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Button::new("install")
                                            .primary()
                                            .disabled(
                                                !can_install
                                                    || (is_managed && !differs_from_installed),
                                            )
                                            .label(match (&busy, is_managed, differs_from_installed)
                                            {
                                                (Some(step), _, _) => step.clone(),
                                                (None, true, true) => {
                                                    format!("Switch to {selected_tag}")
                                                }
                                                (None, true, false) => {
                                                    format!("{selected_tag} installed")
                                                }
                                                (None, false, _) => match selected.is_some() {
                                                    true => format!("Install {selected_tag}"),
                                                    false => "Checking for releases…".to_string(),
                                                },
                                            })
                                            .on_click(cx.listener(
                                                move |this, _: &ClickEvent, _, cx| {
                                                    let id = id_for_install.clone();
                                                    this.state.update(cx, |state, cx| {
                                                        state.install(&id, cx)
                                                    });
                                                },
                                            )),
                                    )
                                    .when(is_managed, |this| {
                                        this.child(
                                            Button::new("uninstall")
                                                .danger()
                                                .disabled(busy.is_some())
                                                .label("Uninstall")
                                                .on_click(cx.listener(
                                                    move |this, _: &ClickEvent, _, cx| {
                                                        let id = id_for_uninstall.clone();
                                                        this.state.update(cx, |state, cx| {
                                                            state.uninstall(&id, cx)
                                                        });
                                                    },
                                                )),
                                        )
                                    })
                                    .child(
                                        Button::new("open-dir-hero")
                                            .ghost()
                                            .icon(IconName::FolderOpen)
                                            .label("Open folder")
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _, cx| {
                                                    this.open_target_dir(cx);
                                                },
                                            )),
                                    ),
                            ),
                    ),
            )
            .child({
                // Left-hand field labels, per the canvas's 130px label column.
                let label_color = cx.theme().secondary_foreground;
                let flabel = move |text: &'static str| {
                    div()
                        .w(px(130.))
                        .flex_shrink_0()
                        .text_sm()
                        .text_color(label_color)
                        .child(text)
                };
                crate::views::ui::section("Installation", cx)
                    .child(
                        h_flex()
                            .gap_2p5()
                            .items_center()
                            .child(flabel("Version"))
                            .children(self.version_select.as_ref().map(|state| {
                                // Select's own wrapper is size_full, which
                                // would stretch across the row and shove the
                                // release date to the far edge; box it in.
                                div()
                                    .flex_none()
                                    .w(px(220.))
                                    .child(Select::new(state).small())
                            }))
                            .when(!selected_published.is_empty(), |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(crate::theme::tokens::faint_text())
                                        .child(format!("released {selected_published}")),
                                )
                            }),
                    )
                    .when(!selected_notes.is_empty(), |this| {
                        this.child(
                            h_flex()
                                .gap_2p5()
                                .items_start()
                                .child(flabel("What's new").pt_2())
                                .child(
                                    v_flex()
                                        .id("changelog")
                                        .p_3()
                                        .gap_1p5()
                                        .max_h(px(120.))
                                        .flex_1()
                                        .overflow_hidden()
                                        .rounded(px(10.))
                                        .border_1()
                                        .border_color(crate::theme::tokens::card_border())
                                        .bg(crate::theme::tokens::inner_bg())
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .child(format!("What's new in {selected_tag}")),
                                        )
                                        .children(selected_notes.iter().map(|line| {
                                            use opti_core::text::NoteLine;
                                            match line {
                                                NoteLine::Heading { level, text } => div()
                                                    .pt_1()
                                                    .text_sm()
                                                    .font_weight(if *level <= 2 {
                                                        gpui::FontWeight::SEMIBOLD
                                                    } else {
                                                        gpui::FontWeight::MEDIUM
                                                    })
                                                    .text_color(cx.theme().foreground)
                                                    .child(text.clone())
                                                    .into_any_element(),
                                                NoteLine::Bullet { indent, text } => h_flex()
                                                    .gap_1p5()
                                                    .items_start()
                                                    .pl(px(8. + *indent as f32 * 14.))
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(cx.theme().primary)
                                                            .child("•"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .flex_1()
                                                            .text_color(
                                                                cx.theme().muted_foreground,
                                                            )
                                                            .child(text.clone()),
                                                    )
                                                    .into_any_element(),
                                                NoteLine::Text(text) => div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(text.clone())
                                                    .into_any_element(),
                                                // Paragraph separator.
                                                NoteLine::Blank => {
                                                    div().h(px(6.)).into_any_element()
                                                }
                                            }
                                        }))
                                        .overflow_y_scrollbar(),
                                ),
                        )
                    })
                    .child(
                        h_flex()
                            .gap_2p5()
                            .items_start()
                            .child(flabel("Load as").pt_1())
                            .child(
                                v_flex()
                                    .gap_1p5()
                                    .flex_1()
                                    .child(
                                        ButtonGroup::new("proxy-name")
                                            .outline()
                                            .compact()
                                            .disabled(is_managed || busy.is_some())
                                            .children(PROXY_DLL_NAMES.iter().enumerate().map(
                                                |(ix, name)| {
                                                    Button::new(("proxy", ix))
                                                        .label(*name)
                                                        .selected(ix == selected_proxy)
                                                },
                                            ))
                                            .on_click(cx.listener(
                                                |this, clicks: &Vec<usize>, _, cx| {
                                                    let Some(&ix) = clicks.first() else {
                                                        return;
                                                    };
                                                    let Some(name) = PROXY_DLL_NAMES.get(ix)
                                                    else {
                                                        return;
                                                    };
                                                    let id = this.game_id.clone();
                                                    this.state.update(cx, |state, cx| {
                                                        state.set_proxy_name(&id, name, cx)
                                                    });
                                                },
                                            )),
                                    )
                                    .child(crate::views::ui::hint(
                                        "The DLL name the game loads. dxgi.dll suits most \
                                         games; switch if the game already ships one.",
                                        cx,
                                    )),
                            ),
                    )
                    .children(self.dir_input.as_ref().map(|input| {
                        h_flex()
                            .gap_2p5()
                            .items_center()
                            .child(flabel("Directory"))
                            .child(Input::new(input).small().flex_1())
                            .child(
                                Button::new("apply-dir")
                                    .small()
                                    .outline()
                                    .label("Change")
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
                    }))
            })
            .child(
                crate::views::ui::section("Compatibility", cx)
                    .child(
                        h_flex()
                            .gap_3()
                            .items_start()
                            .child(
                                // What the game ships: the inputs OptiScaler can hook.
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1p5()
                                    .p_3()
                                    .rounded(px(10.))
                                    .border_1()
                                    .border_color(crate::theme::tokens::card_border())
                                    .bg(crate::theme::tokens::inner_bg())
                                    .map(|this| {
                                        if upscaler_techs.is_empty() {
                                            this.child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(cx.theme().warning)
                                                    .child("No upscaler DLLs found"),
                                            )
                                            .child(crate::views::ui::hint(
                                                "OptiScaler hooks a game's existing \
                                                 DLSS/FSR/XeSS inputs, so it will most \
                                                 likely do nothing here.",
                                                cx,
                                            ))
                                        } else {
                                            let labels = upscaler_techs
                                                .iter()
                                                .map(|tech| tech.label())
                                                .collect::<Vec<_>>()
                                                .join(" and ");
                                            this.child(
                                                h_flex()
                                                    .gap_1p5()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_color(
                                                                crate::theme::tokens::success_pill_text(),
                                                            )
                                                            .child(
                                                                Icon::new(IconName::Check)
                                                                    .xsmall(),
                                                            ),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(
                                                                gpui::FontWeight::SEMIBOLD,
                                                            )
                                                            .child(format!("Ships {labels}")),
                                                    ),
                                            )
                                            .children(upscaler_files.iter().take(3).map(
                                                |detection| {
                                                    crate::views::ui::hint(
                                                        format!(
                                                            "{}: {}",
                                                            detection.tech.label(),
                                                            detection.file.display()
                                                        ),
                                                        cx,
                                                    )
                                                },
                                            ))
                                        }
                                    }),
                            )
                            .child(
                                // Anti-cheat verdict beside it, green or red.
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1p5()
                                    .p_3()
                                    .rounded(px(10.))
                                    .border_1()
                                    .map(|this| {
                                        if anticheat.is_empty() {
                                            this.border_color(
                                                crate::theme::tokens::card_border(),
                                            )
                                            .bg(crate::theme::tokens::inner_bg())
                                            .child(
                                                h_flex()
                                                    .gap_1p5()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_color(
                                                                crate::theme::tokens::success_pill_text(),
                                                            )
                                                            .child(
                                                                Icon::new(IconName::Check)
                                                                    .xsmall(),
                                                            ),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(
                                                                gpui::FontWeight::SEMIBOLD,
                                                            )
                                                            .child("No anti-cheat found"),
                                                    ),
                                            )
                                            .child(crate::views::ui::hint(
                                                "Does not cover server-side systems such as \
                                                 VAC — avoid OptiScaler in anything you play \
                                                 online.",
                                                cx,
                                            ))
                                        } else {
                                            this.border_color(
                                                crate::theme::tokens::danger_pill_border(),
                                            )
                                            .bg(crate::theme::tokens::danger_pill_bg())
                                            .child(
                                                h_flex()
                                                    .gap_1p5()
                                                    .items_center()
                                                    .text_color(
                                                        crate::theme::tokens::danger_pill_text(),
                                                    )
                                                    .child(
                                                        Icon::new(IconName::TriangleAlert)
                                                            .xsmall(),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(
                                                                gpui::FontWeight::SEMIBOLD,
                                                            )
                                                            .child(format!(
                                                                "{anticheat_names} detected"
                                                            )),
                                                    ),
                                            )
                                            .child(crate::views::ui::hint(
                                                "Installing OptiScaler here can get your \
                                                 account banned.",
                                                cx,
                                            ))
                                            .children(anticheat.iter().map(|detection| {
                                                crate::views::ui::hint(
                                                    format!(
                                                        "{}: {}",
                                                        detection.name,
                                                        detection.evidence.display()
                                                    ),
                                                    cx,
                                                )
                                            }))
                                        }
                                    }),
                            ),
                    )
                    .when_some(gpu, |this, gpu| {
                        let needs_spoofing = gpu.vendor.needs_spoofing();
                        this.child(
                            h_flex()
                                .gap_3()
                                .items_center()
                                .justify_between()
                                .p_3()
                                .rounded(px(10.))
                                .border_1()
                                .border_color(crate::theme::tokens::card_border())
                                .bg(crate::theme::tokens::inner_bg())
                                .child(
                                    v_flex()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .child(format!("GPU: {gpu}")),
                                        )
                                        .child(crate::views::ui::hint(
                                            if needs_spoofing {
                                                "Use DLSS inputs — spoofs an Nvidia GPU so \
                                                 games expose their DLSS options. Turn off \
                                                 only if spoofing causes problems here."
                                            } else {
                                                "Nvidia GPU — DLSS inputs work natively, no \
                                                 spoofing needed."
                                            },
                                            cx,
                                        )),
                                )
                                .when(needs_spoofing, |this| {
                                    this.child(
                                        Switch::new("dlss-inputs")
                                            .checked(dlss_inputs)
                                            .on_click(cx.listener(
                                                |this, checked: &bool, _, cx| {
                                                    let id = this.game_id.clone();
                                                    let enabled = *checked;
                                                    this.state.update(cx, |state, cx| {
                                                        state.set_dlss_inputs(&id, enabled, cx)
                                                    });
                                                },
                                            )),
                                    )
                                }),
                        )
                    })
                    .when_some(optipatcher_supported, |this, supported_exe| {
                        let can_add = is_managed && busy.is_none() && !optipatcher_installed;
                        this.child(
                            v_flex()
                                .gap_1p5()
                                .p_3()
                                .rounded(px(10.))
                                .border_1()
                                .border_color(crate::theme::tokens::accent_panel_border())
                                .bg(crate::theme::tokens::accent_panel_bg())
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            v_flex()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                                        .text_color(cx.theme().accent_foreground)
                                                        .child(if optipatcher_installed {
                                                            "OptiPatcher installed".to_string()
                                                        } else {
                                                            "OptiPatcher supported".to_string()
                                                        }),
                                                )
                                                .child(crate::views::ui::hint(
                                                    format!(
                                                        "This game ({supported_exe}) is on \
                                                         OptiPatcher's list: it unlocks DLSS \
                                                         and DLSS-FG inputs without GPU \
                                                         spoofing or its overhead."
                                                    ),
                                                    cx,
                                                )),
                                        )
                                        .when(!optipatcher_installed, |this| {
                                            this.child(
                                                Button::new("add-optipatcher")
                                                    .small()
                                                    .outline()
                                                    .label("Install OptiPatcher")
                                                    .disabled(!can_add)
                                                    .on_click(cx.listener(
                                                        |this, _: &ClickEvent, _, cx| {
                                                            let id = this.game_id.clone();
                                                            this.state.update(cx, |state, cx| {
                                                                state.install_optipatcher(
                                                                    &id, cx,
                                                                )
                                                            });
                                                        },
                                                    )),
                                            )
                                        }),
                                )
                                .when(!is_managed && !optipatcher_installed, |this| {
                                    this.child(crate::views::ui::hint(
                                        "Install OptiScaler first; OptiPatcher loads \
                                         through it.",
                                        cx,
                                    ))
                                }),
                        )
                    }),
            )
            .when_some(conflicts, |this, files| {
                this.child(
                    v_flex()
                        .gap_1p5()
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
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "OptiScaler will be loaded as {proxy_for_click}. Configure it from \
                         its own overlay in game — press Insert once the game is running."
                    )),
            )
            .overflow_y_scrollbar()
            .into_any_element()
    }
}

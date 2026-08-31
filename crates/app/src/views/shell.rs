use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render, Styled,
    Subscription, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Side, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    v_flex,
};
use opti_core::GameId;

use crate::app_state::AppState;
use crate::views::config_editor::ConfigEditor;
use crate::views::game_detail::{GameDetail, OpenConfig};
use crate::views::game_grid::{GameGrid, GameSelected};
use crate::views::settings_view::SettingsView;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Library,
    /// The detail page for one game, reached by clicking a card.
    GameDetail(GameId),
    /// The `OptiScaler.ini` editor for one game.
    ConfigEditor(GameId),
    Settings,
}

/// The routes reachable from the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavItem {
    Library,
    Settings,
}

impl NavItem {
    fn label(self) -> &'static str {
        match self {
            NavItem::Library => "Library",
            NavItem::Settings => "Settings",
        }
    }

    fn icon(self) -> IconName {
        match self {
            NavItem::Library => IconName::LayoutDashboard,
            NavItem::Settings => IconName::Settings2,
        }
    }

    fn route(self) -> Route {
        match self {
            NavItem::Library => Route::Library,
            NavItem::Settings => Route::Settings,
        }
    }

    /// Whether this sidebar item should look active for `route`. A game's
    /// detail page still belongs to the Library section.
    fn matches(self, route: &Route) -> bool {
        matches!(
            (self, route),
            (
                NavItem::Library,
                Route::Library | Route::GameDetail(_) | Route::ConfigEditor(_)
            ) | (NavItem::Settings, Route::Settings)
        )
    }
}

/// Top-level view: sidebar navigation plus the active route's content.
pub struct Shell {
    state: Entity<AppState>,
    grid: Entity<GameGrid>,
    /// Built on demand for whichever game is open.
    detail: Option<Entity<GameDetail>>,
    config: Option<Entity<ConfigEditor>>,
    settings: Entity<SettingsView>,
    route: Route,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
    /// Kept alive while a detail page is open so its events reach us.
    _detail_subscription: Option<Subscription>,
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = AppState::entity(cx);
        let grid = GameGrid::view(state.clone(), cx);
        let settings = SettingsView::view(state.clone(), window, cx);

        let subscription = cx.subscribe(&grid, |this, _, event: &GameSelected, cx| {
            this.open_game(event.0.clone(), cx);
        });

        Shell {
            state,
            grid,
            detail: None,
            config: None,
            settings,
            route: Route::Library,
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![subscription],
            _detail_subscription: None,
        }
    }

    fn open_game(&mut self, id: GameId, cx: &mut Context<Self>) {
        // Reuse the existing view when reopening the same game.
        if self.detail.as_ref().map(|d| d.read(cx).game_id()) != Some(&id) {
            let detail = GameDetail::view(self.state.clone(), id.clone(), cx);
            self._detail_subscription =
                Some(cx.subscribe(&detail, |this, _, event: &OpenConfig, cx| {
                    this.open_config(event.0.clone(), cx);
                }));
            self.detail = Some(detail);
        }
        self.route = Route::GameDetail(id);
        cx.notify();
    }

    fn open_config(&mut self, id: GameId, cx: &mut Context<Self>) {
        // The ini is re-read on open, so always build a fresh editor.
        self.config = Some(ConfigEditor::view(self.state.clone(), id.clone(), cx));
        self.route = Route::ConfigEditor(id);
        cx.notify();
    }
}

impl Focusable for Shell {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let nav_items = [NavItem::Library, NavItem::Settings];
        let scanning = self.state.read(cx).scan.is_scanning();
        // Both the detail page and the config editor are reached from a game,
        // so both offer a way back.
        let back_target = match &self.route {
            Route::GameDetail(_) => Some(Route::Library),
            Route::ConfigEditor(id) => Some(Route::GameDetail(id.clone())),
            _ => None,
        };

        h_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(
                Sidebar::new(Side::Left)
                    .w(px(240.))
                    .header(
                        SidebarHeader::new()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size_8()
                                    .flex_shrink_0()
                                    .rounded(cx.theme().radius)
                                    .bg(cx.theme().primary)
                                    .text_color(cx.theme().primary_foreground)
                                    .child(Icon::new(IconName::Bot)),
                            )
                            .child(
                                v_flex()
                                    .gap_0()
                                    .text_sm()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child("OptiScaler")
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("Manager"),
                                    ),
                            ),
                    )
                    .child(
                        SidebarGroup::new("Navigate").child(SidebarMenu::new().children(
                            nav_items.into_iter().map(|item| {
                                SidebarMenuItem::new(item.label())
                                    .icon(item.icon())
                                    .active(item.matches(&self.route))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.route = item.route();
                                        cx.notify();
                                    }))
                            }),
                        )),
                    ),
            )
            .child(
                v_flex()
                    // Width comes from flex_1 alone: `size_full` here would
                    // claim the whole window and push content past the edge,
                    // since this column sits beside the sidebar.
                    .flex_1()
                    .h_full()
                    .overflow_hidden()
                    .p_4()
                    .gap_3()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(match back_target {
                                Some(target) => {
                                    let label = match target {
                                        Route::Library => "Back to library",
                                        _ => "Back to game",
                                    };
                                    Button::new("back")
                                        .small()
                                        .ghost()
                                        .icon(IconName::ArrowLeft)
                                        .label(label)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.route = target.clone();
                                            cx.notify();
                                        }))
                                        .into_any_element()
                                }
                                None => div().into_any_element(),
                            })
                            .child(
                                Button::new("rescan")
                                    .small()
                                    .outline()
                                    .label(if scanning { "Scanning…" } else { "Rescan" })
                                    .disabled(scanning)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.update(cx, |state, cx| state.rescan(cx));
                                    })),
                            ),
                    )
                    .child(match &self.route {
                        Route::Library => self.grid.clone().into_any_element(),
                        Route::Settings => self.settings.clone().into_any_element(),
                        Route::GameDetail(_) => match &self.detail {
                            Some(detail) => detail.clone().into_any_element(),
                            None => div().into_any_element(),
                        },
                        Route::ConfigEditor(_) => match &self.config {
                            Some(editor) => editor.clone().into_any_element(),
                            None => div().into_any_element(),
                        },
                    }),
            )
    }
}

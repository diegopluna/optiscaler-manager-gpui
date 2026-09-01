use gpui::{
    App, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, Subscription, Window, div, prelude::FluentBuilder,
    px,
};
use gpui_component::{
    ActiveTheme, IconName, Side, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    v_flex,
};

/// Detail and settings read better as a centered column than full-bleed.
fn centered(content: gpui::AnyElement) -> gpui::AnyElement {
    h_flex()
        .size_full()
        .justify_center()
        .child(
            gpui::div()
                .w_full()
                .max_w(gpui::px(860.))
                .h_full()
                .child(content),
        )
        .into_any_element()
}
use opti_core::GameId;

use crate::app_state::AppState;
use crate::views::game_detail::GameDetail;
use crate::views::game_grid::{GameGrid, GameSelected};
use crate::views::settings_view::SettingsView;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Library,
    /// The detail page for one game, reached by clicking a card.
    GameDetail(GameId),
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
            NavItem::Settings => IconName::Settings,
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
            (NavItem::Library, Route::Library | Route::GameDetail(_))
                | (NavItem::Settings, Route::Settings)
        )
    }
}

/// Top-level view: sidebar navigation plus the active route's content.
pub struct Shell {
    state: Entity<AppState>,
    grid: Entity<GameGrid>,
    /// Built on demand for whichever game is open.
    detail: Option<Entity<GameDetail>>,
    settings: Entity<SettingsView>,
    route: Route,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = AppState::entity(cx);
        let grid = GameGrid::view(state.clone(), window, cx);
        let settings = SettingsView::view(state.clone(), window, cx);

        let subscription = cx.subscribe(&grid, |this, _, event: &GameSelected, cx| {
            this.open_game(event.0.clone(), cx);
        });

        Shell {
            state,
            grid,
            detail: None,
            settings,
            route: Route::Library,
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![subscription],
        }
    }

    fn open_game(&mut self, id: GameId, cx: &mut Context<Self>) {
        // Reuse the existing view when reopening the same game.
        if self.detail.as_ref().map(|d| d.read(cx).game_id()) != Some(&id) {
            self.detail = Some(GameDetail::view(self.state.clone(), id.clone(), cx));
        }
        self.route = Route::GameDetail(id);
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
        let update_available = matches!(
            self.state.read(cx).update,
            crate::app_state::UpdateState::Available(_)
        );
        // Both the detail page and the config editor are reached from a game,
        // so both offer a way back.
        let back_target = match &self.route {
            Route::GameDetail(_) => Some(Route::Library),
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
                            .child(crate::views::ui::logo_mark(34.))
                            .child(
                                v_flex()
                                    .gap_0p5()
                                    .text_sm()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child("OptiScaler"),
                                    )
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
                    )
                    .footer(
                        SidebarFooter::new().child(
                            v_flex()
                                .gap_1()
                                .w_full()
                                .p_2p5()
                                .rounded(px(8.))
                                .bg(crate::theme::tokens::card_bg())
                                .border_1()
                                .border_color(crate::theme::tokens::card_border())
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!("v{}", opti_core::update::CURRENT_VERSION)),
                                )
                                .when(update_available, |this| {
                                    this.child(
                                        h_flex()
                                            .id("update-note")
                                            .gap_1p5()
                                            .items_center()
                                            .cursor_pointer()
                                            .child(
                                                div()
                                                    .w(px(6.))
                                                    .h(px(6.))
                                                    .rounded(px(3.))
                                                    .bg(cx.theme().primary),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().link)
                                                    .child("Update available"),
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.route = Route::Settings;
                                                cx.notify();
                                            })),
                                    )
                                }),
                        ),
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
                    .when_some(back_target, |this, target| {
                        this.child(
                            h_flex().child(
                                Button::new("back")
                                    .small()
                                    .ghost()
                                    .icon(IconName::ArrowLeft)
                                    .label("Back to library")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.route = target.clone();
                                        cx.notify();
                                    })),
                            ),
                        )
                    })
                    .child(match &self.route {
                        Route::Library => self.grid.clone().into_any_element(),
                        Route::Settings => centered(self.settings.clone().into_any_element()),
                        Route::GameDetail(_) => match &self.detail {
                            Some(detail) => centered(detail.clone().into_any_element()),
                            None => div().into_any_element(),
                        },
                    }),
            )
    }
}

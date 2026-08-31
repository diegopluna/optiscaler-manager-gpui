use std::rc::Rc;

use gpui::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Pixels,
    Render, Size, StatefulInteractiveElement, Styled, Window, div, px, size,
};
use gpui_component::{
    ActiveTheme, VirtualListScrollHandle, h_flex,
    scroll::{ScrollableElement, ScrollbarAxis},
    v_flex, v_virtual_list,
};

use crate::app_state::AppState;
use crate::views::game_card::{CARD_GAP, CARD_HEIGHT, CARD_WIDTH, game_card};

/// Emitted when the user picks a game, so the shell can open its detail page.
pub struct GameSelected(pub opti_core::GameId);

impl gpui::EventEmitter<GameSelected> for GameGrid {}

/// The library view: a virtualized grid of game cards.
pub struct GameGrid {
    state: Entity<AppState>,
    scroll_handle: VirtualListScrollHandle,
    /// One entry per rendered row; rebuilt when the game count or column count
    /// changes, since the virtual list sizes rows from this.
    row_sizes: Rc<Vec<Size<Pixels>>>,
    columns: usize,
    filter: String,
}

impl GameGrid {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        // Re-render whenever a scan finishes or the catalog changes.
        cx.observe(&state, |_, _, cx| cx.notify()).detach();

        GameGrid {
            state,
            scroll_handle: VirtualListScrollHandle::new(),
            row_sizes: Rc::new(Vec::new()),
            columns: 1,
            filter: String::new(),
        }
    }

    pub fn view(state: Entity<AppState>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(state, cx))
    }

    /// Indices into `AppState::games` that match the current filter.
    fn visible_games(&self, cx: &App) -> Vec<usize> {
        let games = &self.state.read(cx).games;
        let needle = self.filter.trim().to_lowercase();
        games
            .iter()
            .enumerate()
            .filter(|(_, game)| needle.is_empty() || game.title.to_lowercase().contains(&needle))
            .map(|(ix, _)| ix)
            .collect()
    }

    /// How many cards fit across the content area at the current width.
    fn columns_for(width: Pixels) -> usize {
        let usable = f32::from(width) - CARD_GAP;
        let per_card = CARD_WIDTH + CARD_GAP;
        ((usable / per_card).floor() as usize).max(1)
    }
}

impl Render for GameGrid {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let scanning = state.scan.is_scanning();
        let total_games = state.games.len();

        let visible = self.visible_games(cx);

        // The virtual list needs row sizes up front; recompute them whenever
        // the layout or the number of matching games changes. The width must be
        // the real row width: an oversized value throws off the list's scroll
        // maths, which leaves cards painted where their click targets are not.
        let columns = Self::columns_for(window.viewport_size().width - px(280.));
        let row_count = visible.len().div_ceil(columns.max(1));
        let row_size = size(
            px(columns as f32 * (CARD_WIDTH + CARD_GAP)),
            px(CARD_HEIGHT + CARD_GAP),
        );
        if columns != self.columns || self.row_sizes.len() != row_count {
            self.columns = columns;
            self.row_sizes = Rc::new(vec![row_size; row_count]);
        }

        let games = self.state.clone();
        let entity = cx.entity().clone();

        v_flex()
            .size_full()
            .gap_3()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_lg().child("Library"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(if scanning {
                                "Scanning…".to_string()
                            } else {
                                format!("{total_games} games")
                            }),
                    ),
            )
            .child(
                div().relative().size_full().flex_1().child(
                    v_flex()
                        .id("game-grid")
                        .relative()
                        .size_full()
                        .child(
                            v_virtual_list(
                                entity,
                                "game-rows",
                                self.row_sizes.clone(),
                                move |this, visible_rows, _window, cx| {
                                    let indices = this.visible_games(cx);
                                    let columns = this.columns.max(1);
                                    let games = games.read(cx);

                                    visible_rows
                                        .map(|row| {
                                            let start = row * columns;
                                            let end = (start + columns).min(indices.len());
                                            h_flex().gap(px(CARD_GAP)).pb(px(CARD_GAP)).children(
                                                indices[start..end].iter().map(|&ix| {
                                                    let game = &games.games[ix];
                                                    let id = game.id.clone();
                                                    game_card(
                                                        game,
                                                        games.artwork_for(&game.id),
                                                        games.status_for(&game.id),
                                                        cx,
                                                    )
                                                    .on_click(cx.listener(move |_, _, _, cx| {
                                                        cx.emit(GameSelected(id.clone()));
                                                    }))
                                                }),
                                            )
                                        })
                                        .collect()
                                },
                            )
                            .track_scroll(&self.scroll_handle),
                        )
                        .scrollbar(&self.scroll_handle, ScrollbarAxis::Vertical),
                ),
            )
    }
}

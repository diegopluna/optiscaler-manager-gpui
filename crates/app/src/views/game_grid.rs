use std::rc::Rc;

use gpui::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Pixels,
    Render, Size, StatefulInteractiveElement, Styled, Subscription, Window, div,
    prelude::FluentBuilder, px, size,
};
use gpui_component::{
    ActiveTheme, Disableable, IconName, Selectable, Sizable, VirtualListScrollHandle,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::{ScrollableElement, ScrollbarAxis},
    v_flex, v_virtual_list,
};
use opti_core::Store;

use crate::app_state::AppState;
use crate::views::game_card::{CARD_GAP, CARD_HEIGHT, CARD_WIDTH, game_card};
use crate::views::ui::hint;

/// Emitted when the user picks a game, so the shell can open its detail page.
pub struct GameSelected(pub opti_core::GameId);

impl gpui::EventEmitter<GameSelected> for GameGrid {}

/// The library view: search, store filters, and a virtualized grid of cards.
pub struct GameGrid {
    state: Entity<AppState>,
    scroll_handle: VirtualListScrollHandle,
    search: Entity<InputState>,
    /// One entry per rendered row; rebuilt when the game count or column count
    /// changes, since the virtual list sizes rows from this.
    row_sizes: Rc<Vec<Size<Pixels>>>,
    columns: usize,
    filter: String,
    store_filter: Option<Store>,
    _subscriptions: Vec<Subscription>,
}

impl GameGrid {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Re-render whenever a scan finishes or the catalog changes.
        cx.observe(&state, |_, _, cx| cx.notify()).detach();

        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search your library…"));
        let subscription = cx.subscribe(&search, |this: &mut Self, input, _: &InputEvent, cx| {
            this.filter = input.read(cx).value().to_string();
            cx.notify();
        });

        GameGrid {
            state,
            scroll_handle: VirtualListScrollHandle::new(),
            search,
            row_sizes: Rc::new(Vec::new()),
            columns: 1,
            filter: String::new(),
            store_filter: None,
            _subscriptions: vec![subscription],
        }
    }

    pub fn view(state: Entity<AppState>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(state, window, cx))
    }

    /// Indices into `AppState::games` that match the current filters.
    fn visible_games(&self, cx: &App) -> Vec<usize> {
        let games = &self.state.read(cx).games;
        let needle = self.filter.trim().to_lowercase();
        games
            .iter()
            .enumerate()
            .filter(|(_, game)| {
                self.store_filter.is_none_or(|store| game.store == store)
                    && (needle.is_empty() || game.title.to_lowercase().contains(&needle))
            })
            .map(|(ix, _)| ix)
            .collect()
    }

    /// The stores present in the library, for the filter chips.
    fn stores_present(&self, cx: &App) -> Vec<(Store, usize)> {
        let games = &self.state.read(cx).games;
        [Store::Steam, Store::Epic, Store::Xbox, Store::Manual]
            .into_iter()
            .filter_map(|store| {
                let count = games.iter().filter(|game| game.store == store).count();
                (count > 0).then_some((store, count))
            })
            .collect()
    }

    /// How many cards fit across the content area at the current width.
    fn columns_for(width: Pixels) -> usize {
        let usable = f32::from(width) - CARD_GAP;
        let per_card = CARD_WIDTH + CARD_GAP;
        ((usable / per_card).floor() as usize).max(1)
    }

    fn empty_state(&self, filtered: bool, cx: &Context<Self>) -> gpui::AnyElement {
        let (title, detail) = if filtered {
            (
                "No matches".to_string(),
                "Nothing in the library matches the current search or filter.".to_string(),
            )
        } else {
            (
                "No games found".to_string(),
                "Steam, Epic and Xbox libraries are scanned automatically. GOG, EA \
                 or DRM-free installs can be added under Settings → Game locations."
                    .to_string(),
            )
        };

        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child(crate::views::ui::logo_mark(56.))
            .child(
                div()
                    .text_lg()
                    .text_color(cx.theme().foreground)
                    .child(title),
            )
            .child(
                div()
                    .text_sm()
                    .max_w(px(420.))
                    .text_center()
                    .text_color(cx.theme().muted_foreground)
                    .child(detail),
            )
            .into_any_element()
    }
}

impl Render for GameGrid {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let scanning = state.scan.is_scanning();
        let total_games = state.games.len();

        let visible = self.visible_games(cx);
        let stores = self.stores_present(cx);
        let has_filter = !self.filter.trim().is_empty() || self.store_filter.is_some();

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
                    .gap_3()
                    .items_center()
                    .justify_between()
                    .child(
                        v_flex()
                            .gap_0()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("Library"),
                            )
                            .child(hint(
                                if scanning {
                                    "Scanning…".to_string()
                                } else if visible.len() == total_games {
                                    format!("{total_games} games")
                                } else {
                                    format!("{} of {total_games} games", visible.len())
                                },
                                cx,
                            )),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Input::new(&self.search).small().w(px(260.)))
                            .child(
                                Button::new("rescan")
                                    .small()
                                    .ghost()
                                    .icon(IconName::Undo2)
                                    .label(if scanning { "Scanning…" } else { "Rescan" })
                                    .disabled(scanning)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.update(cx, |state, cx| state.rescan(cx));
                                    })),
                            ),
                    ),
            )
            .when(stores.len() > 1, |this| {
                this.child(
                    h_flex()
                        .gap_1()
                        .child(
                            Button::new("store-all")
                                .small()
                                .ghost()
                                .selected(self.store_filter.is_none())
                                .label(format!("All ({total_games})"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.store_filter = None;
                                    cx.notify();
                                })),
                        )
                        .children(stores.into_iter().map(|(store, count)| {
                            Button::new(store.slug())
                                .small()
                                .ghost()
                                .selected(self.store_filter == Some(store))
                                .label(format!("{} ({count})", store.label()))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.store_filter = if this.store_filter == Some(store) {
                                        None
                                    } else {
                                        Some(store)
                                    };
                                    cx.notify();
                                }))
                        })),
                )
            })
            .child(if visible.is_empty() && !scanning {
                self.empty_state(has_filter && total_games > 0, cx)
            } else {
                div()
                    .relative()
                    .size_full()
                    .flex_1()
                    .child(
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
                                                h_flex()
                                                    .gap(px(CARD_GAP))
                                                    .pb(px(CARD_GAP))
                                                    .children(indices[start..end].iter().map(
                                                        |&ix| {
                                                            let game = &games.games[ix];
                                                            let id = game.id.clone();
                                                            game_card(
                                                                game,
                                                                games.artwork_for(&game.id),
                                                                games.status_for(&game.id),
                                                                cx,
                                                            )
                                                            .on_click(cx.listener(
                                                                move |_, _, _, cx| {
                                                                    cx.emit(GameSelected(
                                                                        id.clone(),
                                                                    ));
                                                                },
                                                            ))
                                                        },
                                                    ))
                                            })
                                            .collect()
                                    },
                                )
                                .track_scroll(&self.scroll_handle),
                            )
                            .scrollbar(&self.scroll_handle, ScrollbarAxis::Vertical),
                    )
                    .into_any_element()
            })
    }
}

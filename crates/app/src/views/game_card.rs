use gpui::{
    App, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Stateful, Styled, div, hsla,
    img, px,
};
use gpui_component::{ActiveTheme, v_flex};
use opti_core::{Game, Store};

pub const CARD_WIDTH: f32 = 180.;
pub const COVER_HEIGHT: f32 = 260.;
pub const CARD_HEIGHT: f32 = COVER_HEIGHT + 46.;
pub const CARD_GAP: f32 = 16.;

/// A stable, pleasant colour per game, used for the placeholder cover so the
/// grid still reads as a catalog before artwork has been fetched.
fn cover_color(title: &str) -> Hsla {
    let hash = title
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    hsla((hash % 360) as f32 / 360., 0.42, 0.38, 1.)
}

fn initials(title: &str) -> String {
    title
        .split_whitespace()
        .filter(|word| word.chars().next().is_some_and(char::is_alphanumeric))
        .take(2)
        .filter_map(|word| word.chars().next())
        .collect::<String>()
        .to_uppercase()
}

fn store_label(store: Store) -> &'static str {
    store.label()
}

/// One game tile. `artwork` is a path to an already-downloaded cover; when it
/// is `None` a generated placeholder is drawn instead.
pub fn game_card(
    game: &Game,
    artwork: Option<&std::path::Path>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let cover = match artwork {
        Some(path) => div()
            .size_full()
            .overflow_hidden()
            .child(img(path.to_path_buf()).size_full())
            .into_any_element(),
        None => v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .bg(cover_color(&game.title))
            .text_color(gpui::white())
            .text_2xl()
            .child(initials(&game.title))
            .into_any_element(),
    };

    v_flex()
        .id(gpui::ElementId::Name(game.id.as_str().to_string().into()))
        .w(px(CARD_WIDTH))
        .h(px(CARD_HEIGHT))
        .gap_1()
        .cursor_pointer()
        .child(
            div()
                .w(px(CARD_WIDTH))
                .h(px(COVER_HEIGHT))
                .rounded(cx.theme().radius)
                .overflow_hidden()
                .border_2()
                .border_color(if selected {
                    cx.theme().primary
                } else {
                    cx.theme().border
                })
                .child(cover),
        )
        .child(
            v_flex()
                .gap_0()
                .child(
                    div()
                        .text_sm()
                        .truncate()
                        .text_color(cx.theme().foreground)
                        .child(game.title.clone()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(store_label(game.store)),
                ),
        )
}

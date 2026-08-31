use gpui::{
    App, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Stateful, Styled, div, hsla,
    img, px,
};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, h_flex, v_flex};
use opti_core::optiscaler::InstallStatus;
use opti_core::{Game, Store};

use crate::app_state::GameStatus;

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

/// A small pill drawn over the cover art.
fn tag(label: String, background: Hsla, foreground: Hsla, icon: Option<IconName>) -> Div {
    h_flex()
        .gap_0p5()
        .items_center()
        .px_1()
        .py_0p5()
        .rounded_sm()
        .bg(background)
        .text_color(foreground)
        .text_xs()
        .children(icon.map(|icon| Icon::new(icon).xsmall()))
        .child(label)
}

/// One game tile. `artwork` is a path to an already-downloaded cover; when it
/// is `None` a generated placeholder is drawn instead. `status` drives the
/// badges that say whether OptiScaler is installed and whether the game ships
/// anti-cheat.
pub fn game_card(
    game: &Game,
    artwork: Option<&std::path::Path>,
    status: Option<&GameStatus>,
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

    let install_tag = match status.map(|status| &status.install) {
        Some(InstallStatus::Managed(manifest)) => Some(tag(
            manifest.release_tag.clone(),
            cx.theme().success,
            cx.theme().success_foreground,
            Some(IconName::Check),
        )),
        Some(InstallStatus::Unmanaged { .. }) => Some(tag(
            "Manual".to_string(),
            cx.theme().warning,
            cx.theme().warning_foreground,
            None,
        )),
        _ => None,
    };

    // Anti-cheat is the one thing worth interrupting the browse for: installing
    // OptiScaler into a protected game risks a ban.
    let anticheat_tag = status.filter(|status| status.has_anticheat()).map(|_| {
        tag(
            "Anti-cheat".to_string(),
            cx.theme().danger,
            cx.theme().danger_foreground,
            Some(IconName::TriangleAlert),
        )
    });

    // Which upscalers the game itself ships — where OptiScaler has inputs to
    // hook. Quiet styling: information, not a warning.
    let tech_tags: Vec<Div> = status
        .map(|status| opti_core::upscalers::techs(&status.upscalers))
        .unwrap_or_default()
        .into_iter()
        .map(|tech| {
            tag(
                tech.label().to_string(),
                cx.theme().secondary,
                cx.theme().secondary_foreground,
                None,
            )
        })
        .collect();

    v_flex()
        .id(gpui::ElementId::Name(game.id.as_str().to_string().into()))
        .w(px(CARD_WIDTH))
        .h(px(CARD_HEIGHT))
        .gap_1()
        .cursor_pointer()
        .child(
            div()
                .relative()
                .w(px(CARD_WIDTH))
                .h(px(COVER_HEIGHT))
                .rounded(cx.theme().radius)
                .overflow_hidden()
                .border_1()
                .border_color(cx.theme().border)
                .child(cover)
                .child(
                    v_flex()
                        .absolute()
                        .top_1()
                        .left_1()
                        .right_1()
                        .gap_1()
                        .items_start()
                        .children(anticheat_tag)
                        .children(install_tag),
                )
                .child(
                    h_flex()
                        .absolute()
                        .bottom_1()
                        .left_1()
                        .gap_1()
                        .children(tech_tags),
                ),
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

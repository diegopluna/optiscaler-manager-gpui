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
pub const CARD_HEIGHT: f32 = COVER_HEIGHT + 52.;
pub const CARD_GAP: f32 = 18.;

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
        .gap_1()
        .items_center()
        .h(px(22.))
        .px_2()
        .rounded(px(6.))
        .bg(background)
        .text_color(foreground)
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
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
            // Rounded on the image itself: the overflow clip is rectangular,
            // so a square image would paint over the card's rounded corners.
            .child(img(path.to_path_buf()).size_full().rounded(px(9.)))
            .into_any_element(),
        None => v_flex()
            .size_full()
            .rounded(px(9.))
            .items_center()
            .justify_center()
            .bg(cover_color(&game.title))
            .text_color(gpui::white())
            .text_2xl()
            .child(initials(&game.title))
            .into_any_element(),
    };

    use crate::theme::tokens;
    let install_tag = match status.map(|status| &status.install) {
        Some(InstallStatus::Managed(manifest)) => Some(
            tag(
                manifest.release_tag.clone(),
                tokens::success_pill_bg(),
                tokens::success_pill_text(),
                Some(IconName::Check),
            )
            .border_1()
            .border_color(tokens::success_pill_border()),
        ),
        Some(InstallStatus::Unmanaged { .. }) => Some(
            tag(
                "Manual".to_string(),
                tokens::danger_pill_bg(),
                cx.theme().warning,
                None,
            )
            .border_1()
            .border_color(cx.theme().warning.opacity(0.4)),
        ),
        _ => None,
    };

    // Anti-cheat is the one thing worth interrupting the browse for: installing
    // OptiScaler into a protected game risks a ban.
    let anticheat_tag = status.filter(|status| status.has_anticheat()).map(|_| {
        tag(
            "Anti-cheat".to_string(),
            tokens::danger_pill_bg(),
            tokens::danger_pill_text(),
            Some(IconName::TriangleAlert),
        )
        .border_1()
        .border_color(tokens::danger_pill_border())
    });

    // Which upscalers the game itself ships — where OptiScaler has inputs to
    // hook. Quiet dark pills so they read on any cover art.
    let tech_tags: Vec<Div> = status
        .map(|status| opti_core::upscalers::techs(&status.upscalers))
        .unwrap_or_default()
        .into_iter()
        .map(|tech| crate::views::ui::art_pill(tech.label()))
        .collect();

    v_flex()
        .id(gpui::ElementId::Name(game.id.as_str().to_string().into()))
        .group("game-card")
        .w(px(CARD_WIDTH))
        .h(px(CARD_HEIGHT))
        .gap_2()
        .cursor_pointer()
        .child(
            div()
                .relative()
                .w(px(CARD_WIDTH))
                .h(px(COVER_HEIGHT))
                .rounded(px(10.))
                .overflow_hidden()
                .border_1()
                .border_color(crate::theme::tokens::cover_border())
                .shadow_lg()
                .group_hover("game-card", |this| this.border_color(cx.theme().primary))
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
                .gap_1()
                .px(px(2.))
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
                        .text_color(crate::theme::tokens::faint_text())
                        .child(store_label(game.store)),
                ),
        )
}

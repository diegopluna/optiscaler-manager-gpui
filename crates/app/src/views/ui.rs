//! Small shared pieces of the app's visual language.

use gpui::{App, Div, ParentElement, Styled, div, hsla, px};
use gpui_component::{ActiveTheme, h_flex, v_flex};

/// A titled content card: the unit every page is built from, so spacing and
/// borders stay consistent everywhere. Titles render as small uppercase
/// labels, per the design canvas.
pub fn section(title: impl Into<String>, cx: &App) -> Div {
    let _ = cx;
    v_flex()
        .w_full()
        .gap_2p5()
        .p_4()
        .rounded(px(12.))
        .border_1()
        .border_color(crate::theme::tokens::card_border())
        .bg(crate::theme::tokens::card_bg())
        .child(
            div()
                .text_size(px(11.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(crate::theme::tokens::section_label())
                .child(title.into().to_uppercase()),
        )
}

/// A small neutral chip, for metadata like the store or shipped upscalers.
pub fn chip(label: impl Into<String>, cx: &App) -> Div {
    h_flex()
        .h(px(22.))
        .px_2()
        .rounded(px(6.))
        .bg(cx.theme().secondary)
        .border_1()
        .border_color(cx.theme().border)
        .text_color(cx.theme().muted_foreground)
        .text_xs()
        .child(label.into())
}

/// One line of muted helper text under a control.
pub fn hint(text: impl Into<String>, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// The app mark: three squares stepping up in size and opacity — the same
/// upscaling motif as the application icon, drawn live so it always matches
/// the theme radius and scales crisply.
pub fn logo_mark(size: f32) -> Div {
    let square = |s: f32, alpha: f32, x: f32, y: f32| {
        div()
            .absolute()
            .left(px(x * size))
            .top(px(y * size))
            .w(px(s * size))
            .h(px(s * size))
            .rounded(px(s * size * 0.3))
            .bg(hsla(0., 0., 1., alpha))
    };

    div()
        .relative()
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .rounded(px(size * 0.22))
        .bg(gpui::linear_gradient(
            135.,
            gpui::linear_color_stop(hsla(263. / 360., 0.70, 0.50, 1.), 0.),
            gpui::linear_color_stop(hsla(244. / 360., 0.55, 0.41, 1.), 1.),
        ))
        .child(square(0.17, 0.55, 0.19, 0.64))
        .child(square(0.26, 0.78, 0.36, 0.38))
        .child(square(0.39, 1.0, 0.52, 0.09))
}

/// A quiet pill for information badges over artwork: readable on any cover.
pub fn art_pill(label: impl Into<String>) -> Div {
    h_flex()
        .px_1p5()
        .py_0p5()
        .rounded_sm()
        .bg(hsla(0., 0., 0., 0.60))
        .text_color(hsla(0., 0., 1., 0.92))
        .text_xs()
        .child(label.into())
}

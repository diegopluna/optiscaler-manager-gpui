//! The app's dark palette, from the design canvas: near-black surfaces with
//! a violet cast and one brightened accent.
//!
//! Rather than styling every component by hand, the gpui-component global
//! theme is overridden so stock controls (buttons, inputs, selects, the
//! sidebar) follow the palette. `Theme::change` rebuilds colors from the
//! built-in themes, so [`apply`] must run again after every mode change; in
//! light mode the built-in theme is left untouched.

use gpui::{App, Hsla, rgb, rgba};
use gpui_component::{Theme, ThemeMode};

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn ca(hex_with_alpha: u32) -> Hsla {
    rgba(hex_with_alpha).into()
}

/// Applies the canvas palette when the active mode is dark. Call after
/// `gpui_component::init` and again after every `Theme::change`.
pub fn apply(cx: &mut App) {
    if Theme::global(cx).mode != ThemeMode::Dark {
        return;
    }

    let theme = Theme::global_mut(cx);
    let colors = &mut theme.colors;

    colors.background = c(0x0b0b10);
    colors.foreground = c(0xececf4);
    colors.border = c(0x23232e);
    colors.input = c(0x23232e);
    colors.ring = c(0x7c5cfc);

    colors.secondary = c(0x14141d);
    colors.secondary_foreground = c(0xb9b9c9);
    colors.secondary_hover = c(0x1a1a25);
    colors.secondary_active = c(0x20202c);
    colors.muted = c(0x14141d);
    colors.muted_foreground = c(0x8b8b9e);

    colors.primary = c(0x7c5cfc);
    colors.primary_foreground = c(0xffffff);
    colors.primary_hover = c(0x8d71fd);
    colors.primary_active = c(0x6b4be0);

    colors.accent = ca(0x7c5cfc29);
    colors.accent_foreground = c(0xd6c9ff);

    colors.sidebar = c(0x0e0e15);
    colors.sidebar_border = c(0x1d1d28);
    colors.sidebar_foreground = c(0xb9b9c9);
    colors.sidebar_accent = ca(0x7c5cfc24);
    colors.sidebar_accent_foreground = c(0xd6c9ff);
    colors.sidebar_primary = c(0x7c5cfc);
    colors.sidebar_primary_foreground = c(0xffffff);

    colors.list = c(0x12121a);
    colors.list_hover = ca(0xffffff0a);
    colors.list_active = ca(0x7c5cfc1f);
    colors.popover = c(0x12121a);
    colors.popover_foreground = c(0xececf4);

    colors.danger = c(0xe5484d);
    colors.danger_foreground = c(0xffffff);
    colors.success = c(0x2f9e6f);
    colors.success_foreground = c(0xeafff5);
    colors.warning = c(0xd9a53f);
    colors.warning_foreground = c(0x1a1206);

    colors.link = c(0xa78bfa);
    colors.caret = c(0x7c5cfc);
    colors.selection = ca(0x7c5cfc4d);
    colors.scrollbar_thumb = c(0x2a2a38);
    colors.skeleton = c(0x1a1a25);
}

/// Surface and text tokens for the app's own components — the values the
/// design canvas uses that have no theme slot.
pub mod tokens {
    use gpui::Hsla;

    pub fn card_bg() -> Hsla {
        super::c(0x12121a)
    }
    pub fn card_border() -> Hsla {
        super::c(0x1d1d28)
    }
    pub fn inner_bg() -> Hsla {
        super::c(0x0e0e15)
    }
    pub fn cover_border() -> Hsla {
        super::c(0x2c2c3a)
    }
    pub fn faint_text() -> Hsla {
        super::c(0x6e6e80)
    }
    pub fn section_label() -> Hsla {
        super::c(0x5b5b6e)
    }
    /// Installed pill: deep green glass with a bright rim and text.
    pub fn success_pill_bg() -> Hsla {
        super::ca(0x143c28d9)
    }
    pub fn success_pill_border() -> Hsla {
        super::ca(0x34d39966)
    }
    pub fn success_pill_text() -> Hsla {
        super::c(0x6ee7b7)
    }
    /// Anti-cheat pill: deep red glass.
    pub fn danger_pill_bg() -> Hsla {
        super::ca(0x461616e0)
    }
    pub fn danger_pill_border() -> Hsla {
        super::ca(0xf8717166)
    }
    pub fn danger_pill_text() -> Hsla {
        super::c(0xfca5a5)
    }
    /// The OptiPatcher / accent-tinted panel.
    pub fn accent_panel_bg() -> Hsla {
        super::ca(0x7c5cfc12)
    }
    pub fn accent_panel_border() -> Hsla {
        super::ca(0x7c5cfc40)
    }
}

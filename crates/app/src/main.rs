// Hide the console window that Windows would otherwise attach to a GUI build.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app_state;
mod views;

use gpui::{
    AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};
use gpui_component::Root;

use crate::views::shell::Shell;

fn main() {
    env_logger::init();

    let app = Application::new().with_assets(gpui_component_assets::Assets);

    app.run(move |cx| {
        gpui_component::init(cx);

        // Apply the saved theme before the window opens; absent means follow
        // the system appearance, which is the default behaviour.
        match opti_core::Settings::load().theme.as_deref() {
            Some("light") => {
                gpui_component::Theme::change(gpui_component::ThemeMode::Light, None, cx)
            }
            Some("dark") => {
                gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx)
            }
            _ => {}
        }

        cx.spawn(async move |cx| {
            let bounds = cx.update(|cx| Bounds::centered(None, size(px(1280.), px(820.)), cx))?;
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("OptiScaler Manager".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let shell = cx.new(|cx| Shell::new(window, cx));
                    // The first level inside the window must be a Root.
                    cx.new(|cx| Root::new(shell, window, cx))
                },
            )?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}

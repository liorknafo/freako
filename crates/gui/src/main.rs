mod app;
mod ui;

use iced::Size;

fn main() -> iced::Result {
    tracing_subscriber::fmt::init();

    let config = freako_core::config::load_config().unwrap_or_default();
    let window_size = Size::new(
        config.ui.window_width as f32,
        config.ui.window_height as f32,
    );

    iced::application(app::App::boot, app::App::update, app::App::view)
        .subscription(app::App::subscription)
        .theme(app::App::theme)
        .title(|_state: &app::App| "freako".to_string())
        .window_size(window_size)
        .run()
}

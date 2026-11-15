use server_picker_rs::app::{AppFlags, ServerPickerApp};

fn main() -> iced::Result {
    tracing_subscriber::fmt::init();

    let flags = tokio::runtime::Runtime::new()
        .expect("tokio runtime")
        .block_on(AppFlags::new())
        .expect("failed to initialize application");

    iced::application(
        move || ServerPickerApp::new(flags.clone()),
        ServerPickerApp::update,
        ServerPickerApp::view,
    )
    .title(ServerPickerApp::title)
    .theme(ServerPickerApp::theme)
    .subscription(ServerPickerApp::subscription)
    .window(iced::window::Settings {
        decorations: false,
        size: iced::Size::new(1100.0, 700.0),
        min_size: Some(iced::Size::new(600.0, 400.0)),
        ..iced::window::Settings::default()
    })
    .run()
}

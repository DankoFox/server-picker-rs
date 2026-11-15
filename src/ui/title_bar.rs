use crate::app::{Message, ServerPickerApp};
use iced::widget::{button, container, mouse_area, row, text, Row};
use iced::{Alignment, Element, Length, Padding, Theme};

const TITLE_BAR_HEIGHT: f32 = 34.0;
const BUTTON_WIDTH: f32 = 44.0;

pub fn view<'a>(app: &'a ServerPickerApp) -> Element<'a, Message> {
    let title = mouse_area(
        container(text("ServerPickerX").size(14))
            .align_x(Alignment::Start)
            .align_y(Alignment::Center)
            .padding(Padding::from([0, 16]))
            .height(Length::Fill)
            .width(Length::Fill),
    )
    .on_press(Message::DragWindow);

    let window_controls = row![
        title_bar_button("−", Message::MinimizeWindow),
        title_bar_button(
            if app.is_maximized { "❐" } else { "□" },
            Message::ToggleMaximizeWindow
        ),
        title_bar_button("×", Message::CloseWindow),
    ]
    .spacing(0)
    .height(Length::Fill);

    Row::new()
        .push(title)
        .push(window_controls)
        .spacing(0)
        .align_y(Alignment::Center)
        .height(TITLE_BAR_HEIGHT)
        .width(Length::Fill)
        .into()
}

fn title_bar_button<'a>(label: &'a str, message: Message) -> Element<'a, Message> {
    button(
        container(text(label).size(14))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(message)
    .width(Length::Fixed(BUTTON_WIDTH))
    .height(Length::Fill)
    .style(|theme: &Theme, status: button::Status| {
        let palette = theme.palette();
        let background = match status {
            button::Status::Hovered => palette.text.scale_alpha(0.06),
            _ => palette.background,
        };
        button::Style {
            background: Some(background.into()),
            text_color: palette.text,
            border: iced::Border::default(),
            shadow: iced::Shadow::default(),
            snap: false,
        }
    })
    .into()
}

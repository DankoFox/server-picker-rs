use crate::app::{Message, ServerPickerApp};
use iced::widget::{button, checkbox, text, Column, Container, Row, Scrollable, Svg, Text};
use iced::{Alignment, Element, Length, Padding, Theme};
use std::collections::HashMap;
use std::sync::LazyLock;

const HEADER_HEIGHT: u16 = 32;
const ROW_HEIGHT: u16 = 36;

pub fn view<'a>(app: &'a ServerPickerApp) -> Element<'a, Message> {
    let headers = header_row();
    let rows: Column<'a, Message> = app.filtered_servers().into_iter().enumerate().fold(
        Column::new().spacing(1).width(Length::Fill),
        |column, (index, server)| {
            let key = app.server_key(server);
            let is_selected = app.selected_servers.contains(&key);
            column.push(data_row(app, server, index, key, is_selected))
        },
    );

    let table = Column::new()
        .push(headers)
        .push(
            Scrollable::new(rows)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill);

    Container::new(table)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(0)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            iced::widget::container::Style {
                background: Some(palette.background.into()),
                ..Default::default()
            }
        })
        .into()
}

fn header_row<'a>() -> Element<'a, Message> {
    Row::new()
        .push(header_cell("", Length::Fixed(40.0)))
        .push(header_cell("Flag", Length::Fixed(80.0)))
        .push(header_cell("ID", Length::Fixed(80.0)))
        .push(header_cell("Name", Length::Fill))
        .push(header_cell("Avg Ping", Length::Fixed(80.0)))
        .push(header_cell("Loss", Length::Fixed(60.0)))
        .push(header_cell("State", Length::Fixed(60.0)))
        .width(Length::Fill)
        .height(HEADER_HEIGHT)
        .into()
}

fn header_cell<'a>(label: &'a str, width: Length) -> Element<'a, Message> {
    Container::new(Text::new(label).size(13))
        .width(width)
        .height(HEADER_HEIGHT)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            iced::widget::container::Style {
                background: Some(palette.primary.into()),
                ..Default::default()
            }
        })
        .into()
}

fn data_row<'a>(
    app: &'a ServerPickerApp,
    server: &'a crate::core::server::Server,
    _index: usize,
    key: String,
    is_selected: bool,
) -> Element<'a, Message> {
    let key_for_checkbox = key.clone();
    let checkbox_widget = checkbox("", is_selected)
        .on_toggle(move |_| Message::ToggleServerSelection(key_for_checkbox.clone()));

    let description_text = server.description.clone();
    let description: Element<'a, Message> = Container::new(Text::new(description_text).size(13))
        .width(Length::Fill)
        .height(ROW_HEIGHT)
        .align_y(Alignment::Center)
        .padding(Padding::from([0, 4]))
        .into();

    let row = Row::new()
        .push(data_cell(checkbox_widget.into(), Length::Fixed(40.0)))
        .push(data_cell(flag_cell(&server.flag), Length::Fixed(80.0)))
        .push(data_cell(
            Text::new(server.name.clone()).size(13).into(),
            Length::Fixed(80.0),
        ))
        .push(description)
        .push(data_cell(
            Text::new(
                server
                    .ping_ms
                    .map(|ms| format!("{} ms", ms))
                    .unwrap_or_else(|| "-".to_string()),
            )
            .size(13)
            .into(),
            Length::Fixed(80.0),
        ))
        .push(data_cell(
            Text::new(
                server
                    .packet_loss_percent
                    .map(|p| format!("{}%", p))
                    .unwrap_or_else(|| "-".to_string()),
            )
            .size(13)
            .into(),
            Length::Fixed(60.0),
        ))
        .push(data_cell(
            firewall_state_text(app, server),
            Length::Fixed(60.0),
        ))
        .width(Length::Fill)
        .height(ROW_HEIGHT);

    button(row)
        .on_press(Message::ToggleServerSelection(key.to_string()))
        .width(Length::Fill)
        .padding(0)
        .style(|theme: &Theme, status: button::Status| {
            let palette = theme.palette();
            let background = match status {
                button::Status::Hovered => palette.primary.scale_alpha(0.2),
                _ => palette.background,
            };
            button::Style {
                background: Some(background.into()),
                text_color: palette.text,
                border: iced::Border::default(),
                shadow: iced::Shadow::default(),
            }
        })
        .into()
}

fn data_cell<'a>(content: Element<'a, Message>, width: Length) -> Element<'a, Message> {
    Container::new(content)
        .width(width)
        .height(ROW_HEIGHT)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .padding(Padding::from([0, 4]))
        .into()
}

fn flag_cell<'a>(code: &'a str) -> Element<'a, Message> {
    match svg_flag(code) {
        Some(svg) => svg.height(Length::Fixed(16.0)).into(),
        None => Container::new(Text::new(code).size(10).font(iced::Font::MONOSPACE))
            .width(Length::Fixed(28.0))
            .height(Length::Fixed(18.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(|theme: &Theme| {
                let palette = theme.palette();
                iced::widget::container::Style {
                    background: Some(palette.primary.scale_alpha(0.15).into()),
                    border: iced::Border {
                        color: palette.primary.scale_alpha(0.4),
                        width: 1.0,
                        radius: iced::border::Radius::from(4.0),
                    },
                    ..Default::default()
                }
            })
            .into(),
    }
}

static FLAG_CACHE: LazyLock<HashMap<String, Vec<u8>>> = LazyLock::new(|| {
    rs_grid_icons::all_flags()
        .filter_map(|(code, uri)| {
            let encoded = uri.strip_prefix("data:image/svg+xml;base64,")?;
            let bytes =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).ok()?;
            Some((code.to_string(), bytes))
        })
        .collect()
});

fn svg_flag(code: &str) -> Option<Svg<'static, Theme>> {
    let bytes = FLAG_CACHE.get(code).cloned()?;
    Some(Svg::new(iced::widget::svg::Handle::from_memory(bytes)))
}

fn firewall_state_text<'a>(
    app: &'a ServerPickerApp,
    server: &crate::core::server::Server,
) -> Element<'a, Message> {
    let is_blocked = server
        .relays
        .iter()
        .any(|relay| app.blocked_relays.contains(&relay.ipv4));
    let (label, color) = if is_blocked {
        ("Blocked", iced::Color::from_rgb(1.0, 0.45, 0.45))
    } else {
        ("Open", iced::Color::from_rgb(0.45, 0.95, 0.45))
    };

    Text::new(label)
        .size(11)
        .style(move |_theme: &Theme| text::Style { color: Some(color) })
        .into()
}

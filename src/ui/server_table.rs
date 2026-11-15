use crate::app::{Message, ServerPickerApp};
use crate::core::server::Server;
use iced::widget::{button, checkbox, text, Column, Container, Row, Scrollable, Svg, Text};
use iced::{Alignment, Element, Length, Padding, Theme};
use std::collections::HashMap;
use std::sync::LazyLock;

const HEADER_HEIGHT: f32 = 32.0;
const ROW_HEIGHT: f32 = 36.0;
const REGION_HEADER_HEIGHT: f32 = 26.0;

pub fn view<'a>(app: &'a ServerPickerApp) -> Element<'a, Message> {
    let headers = header_row();
    let mut servers: Vec<&Server> = app.filtered_servers();
    servers.sort_by(|a, b| {
        a.region_group()
            .cmp(b.region_group())
            .then_with(|| a.region_name().cmp(b.region_name()))
            .then_with(|| a.name.cmp(&b.name))
    });

    let rows: Column<'a, Message> = if app.group_by_region() {
        servers
            .chunk_by(|a, b| a.region_group() == b.region_group())
            .fold(Column::new().spacing(1).width(Length::Fill), |column, group| {
                let region = group.first().map(|s| s.region_group()).unwrap_or("Unknown");
                let group_rows = group.iter().enumerate().fold(
                    Column::new().spacing(1).width(Length::Fill),
                    |column, (index, server)| {
                        push_server_row(app, column, server, index)
                    },
                );
                column
                    .push(region_header(region))
                    .push(group_rows)
            })
    } else {
        servers.into_iter().enumerate().fold(
            Column::new().spacing(1).width(Length::Fill),
            |column, (index, server)| push_server_row(app, column, server, index),
        )
    };

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

fn push_server_row<'a>(
    app: &'a ServerPickerApp,
    column: Column<'a, Message>,
    server: &'a Server,
    index: usize,
) -> Column<'a, Message> {
    let key = app.server_key(server);
    let is_selected = app.selected_servers.contains(&key);
    column.push(data_row(app, server, index, key, is_selected))
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

fn region_header<'a>(region: &'a str) -> Element<'a, Message> {
    Container::new(
        Row::new()
            .push(Text::new(region).size(12))
            .padding(Padding::from([0, 8])),
    )
    .width(Length::Fill)
    .height(REGION_HEADER_HEIGHT)
    .align_y(Alignment::Center)
    .style(|theme: &Theme| {
        let palette = theme.palette();
        iced::widget::container::Style {
            background: Some(palette.background.scale_alpha(0.5).into()),
            ..Default::default()
        }
    })
    .into()
}

fn data_row<'a>(
    app: &'a ServerPickerApp,
    server: &'a Server,
    _index: usize,
    key: String,
    is_selected: bool,
) -> Element<'a, Message> {
    let checkbox_key = key.clone();
    let checkbox_widget = checkbox(is_selected)
        .on_toggle(move |_| Message::ToggleServerSelection(checkbox_key.clone()));

    let description: Element<'a, Message> = Container::new(Text::new(&server.description).size(13))
        .width(Length::Fill)
        .height(ROW_HEIGHT)
        .align_y(Alignment::Center)
        .padding(Padding::from([0, 4]))
        .into();

    let row = Row::new()
        .push(data_cell(checkbox_widget.into(), Length::Fixed(40.0)))
        .push(data_cell(flag_cell(&server.flag), Length::Fixed(80.0)))
        .push(data_cell(
            Text::new(&server.name).size(13).into(),
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
                snap: false,
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
    server: &Server,
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

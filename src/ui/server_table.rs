use crate::app::{Message, ServerPickerApp};
use crate::core::server::Server;
use iced::widget::{
    button, checkbox, text, Column, Container, Row, Scrollable, Space, Svg, Text,
};
use iced::{Alignment, Element, Length, Padding, Theme};
use std::collections::HashMap;
use std::sync::LazyLock;

const HEADER_HEIGHT: f32 = 32.0;
const ROW_HEIGHT: f32 = 36.0;
const REGION_HEADER_HEIGHT: f32 = 26.0;
/// Extra rows rendered above and below the viewport so small scrolls don't
/// reveal blank space.
const OVERSCAN_ROWS: usize = 5;

/// A row in the virtualized table: either a region group header or a server.
enum TableItem<'a> {
    Region(&'a str),
    Server(&'a Server),
}

pub fn view<'a>(app: &'a ServerPickerApp) -> Element<'a, Message> {
    let headers = header_row();
    let servers: Vec<&Server> = app.filtered_servers();

    let items: Vec<TableItem> = if app.group_by_region() {
        let mut items = Vec::new();
        for group in servers.chunk_by(|a, b| a.region_group() == b.region_group()) {
            let region = group.first().map(|s| s.region_group()).unwrap_or("Unknown");
            items.push(TableItem::Region(region));
            for server in group {
                items.push(TableItem::Server(server));
            }
        }
        items
    } else {
        servers.into_iter().map(TableItem::Server).collect()
    };

    let (top_spacer, start, end, bottom_spacer, total_height) =
        visible_slice(app, &items, ROW_HEIGHT, REGION_HEADER_HEIGHT);

    let rows: Column<'a, Message> = items[start..end].iter().enumerate().fold(
        Column::new().spacing(1).width(Length::Fill),
        |column, (index, item)| match item {
            TableItem::Region(region) => column.push(region_header(region)),
            TableItem::Server(server) => push_server_row(app, column, server, index),
        },
    );

    // When every row fits inside the viewport, extend the content so the
    // scrollable area still fills the table instead of leaving blank space.
    let filler_height = if end == items.len() {
        (app.table_viewport_height - total_height).max(0.0)
    } else {
        0.0
    };

    let content = Column::new()
        .push(Space::new().height(Length::Fixed(top_spacer)))
        .push(rows)
        .push(Space::new().height(Length::Fixed(bottom_spacer)))
        .push(Space::new().height(Length::Fixed(filler_height)))
        .width(Length::Fill);

    let table = Column::new()
        .push(headers)
        .push(
            Scrollable::new(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .on_scroll(Message::TableScrolled),
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

/// Compute the range of items that should be rendered for the current scroll
/// position, plus spacer heights to preserve total scrollable size.
fn visible_slice(
    app: &ServerPickerApp,
    items: &[TableItem],
    row_height: f32,
    header_height: f32,
) -> (f32, usize, usize, f32, f32) {
    if items.is_empty() {
        return (0.0, 0, 0, 0.0, 0.0);
    }

    let overscan = OVERSCAN_ROWS as f32 * row_height;
    let visible_start_y = (app.table_scroll_offset - overscan).max(0.0);
    let visible_end_y = app.table_scroll_offset + app.table_viewport_height + overscan;

    // Build cumulative Y positions for each item start.
    let mut y_positions = vec![0.0];
    let mut y = 0.0;
    for item in items {
        y += item_height(item, row_height, header_height);
        y_positions.push(y);
    }
    let total_height = y;

    let start = y_positions
        .iter()
        .position(|&pos| pos >= visible_start_y)
        .unwrap_or(items.len())
        .min(items.len());

    let end = y_positions
        .iter()
        .position(|&pos| pos > visible_end_y)
        .unwrap_or(items.len())
        .min(items.len())
        .max(start);

    let top_spacer = y_positions[start];
    let bottom_spacer = total_height - y_positions[end];

    (top_spacer, start, end, bottom_spacer, total_height)
}

fn item_height(item: &TableItem, row_height: f32, header_height: f32) -> f32 {
    match item {
        TableItem::Region(_) => header_height,
        TableItem::Server(_) => row_height,
    }
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
                background: Some(palette.text.scale_alpha(0.05).into()),
                border: iced::Border {
                    color: palette.text.scale_alpha(0.06),
                    width: 1.0,
                    radius: iced::border::Radius::from(0),
                },
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
                button::Status::Hovered => palette.text.scale_alpha(0.05),
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
                    background: Some(palette.text.scale_alpha(0.06).into()),
                    border: iced::Border {
                        color: palette.text.scale_alpha(0.1),
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

    Text::new(if is_blocked { "Blocked" } else { "Open" })
        .size(11)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            let color = if is_blocked { palette.danger } else { palette.success };
            text::Style { color: Some(color) }
        })
        .into()
}

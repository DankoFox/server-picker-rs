use crate::app::{Message, ServerPickerApp};
use crate::ui::{preset_dialog, server_table, title_bar};
use iced::widget::{
    button, Button, Column, Container, PickList, ProgressBar, Row, Stack, Text, TextInput,
};
use iced::{Alignment, Element, Length, Padding, Theme};

const CONTROL_SPACING: f32 = 4.0;
const SECTION_PADDING: Padding = Padding {
    top: 4.0,
    right: 10.0,
    bottom: 4.0,
    left: 10.0,
};

pub fn view<'a>(app: &'a ServerPickerApp) -> Element<'a, Message> {
    let progress: Element<Message> = if app.pending_operation {
        Container::new(ProgressBar::new(0.0..=1.0, 0.5))
            .height(3)
            .width(Length::Fill)
            .into()
    } else {
        Container::new(Text::new(""))
            .height(3)
            .width(Length::Fill)
            .into()
    };

    let content = Column::new()
        .push(title_bar::view(app))
        .push(control_bar(app))
        .push(progress)
        .push(server_table::view(app))
        .push(status_footer(app))
        .width(Length::Fill)
        .height(Length::Fill);

    if let Some(dialog) = preset_dialog::view(app) {
        Stack::new()
            .push(content)
            .push(
                Container::new(dialog)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .style(|theme: &Theme| {
                        let palette = theme.palette();
                        iced::widget::container::Style {
                            background: Some(palette.background.scale_alpha(0.75).into()),
                            ..Default::default()
                        }
                    }),
            )
            .into()
    } else {
        content.into()
    }
}

fn control_bar<'a>(app: &'a ServerPickerApp) -> Element<'a, Message> {
    let game_modes: Vec<String> = app
        .definitions
        .iter()
        .map(|d| d.game_mode.clone())
        .collect();

    let game_mode_pick = PickList::new(
        game_modes,
        Some(app.current_game_mode.clone()),
        Message::GameModeChanged,
    )
    .width(Length::Fixed(160.0));

    let search = TextInput::new("Search servers...", &app.search_text)
        .on_input(Message::SearchChanged)
        .width(Length::Fixed(140.0));

    let cluster_button = toolbar_button(
        if app.is_clustered {
            "Uncluster"
        } else {
            "Cluster"
        },
        Message::ToggleCluster,
    );

    let preset_names: Vec<String> = app
        .settings
        .settings()
        .presets_for_game_mode(&app.current_game_mode)
        .into_iter()
        .map(|p| p.name.clone())
        .collect();

    let preset_pick = PickList::new(
        preset_names,
        app.active_preset_name.clone(),
        Message::PresetSelected,
    )
    .width(Length::Fixed(110.0))
    .placeholder("Preset");

    let apply_preset = primary_button("Apply", Message::ApplyPreset);
    let save_preset = toolbar_button("Save", Message::SavePreset);
    let manage_presets = toolbar_button("Manage", Message::PresetDialogOpened);

    let ping_button = toolbar_button("Ping", Message::PingAll);
    let block_all = danger_button("Block All", Message::BlockAll);
    let block_selected = danger_button("Block", Message::BlockSelected);
    let unblock_all = success_button("Unblock All", Message::UnblockAll);
    let unblock_selected = success_button("Unblock", Message::UnblockSelected);

    let config_group = Row::new()
        .push(game_mode_pick)
        .push(search)
        .push(cluster_button)
        .spacing(CONTROL_SPACING)
        .align_y(Alignment::Center);

    let preset_group = Row::new()
        .push(preset_pick)
        .push(apply_preset)
        .push(save_preset)
        .push(manage_presets)
        .spacing(CONTROL_SPACING)
        .align_y(Alignment::Center);

    let action_group = Row::new()
        .push(ping_button)
        .push(block_all)
        .push(block_selected)
        .push(unblock_all)
        .push(unblock_selected)
        .spacing(CONTROL_SPACING)
        .align_y(Alignment::Center);

    let top_row = Row::new()
        .push(config_group)
        .push(separator())
        .push(preset_group)
        .spacing(CONTROL_SPACING * 2.0)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    let bottom_row = Row::new()
        .push(action_group)
        .spacing(CONTROL_SPACING * 2.0)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    Container::new(
        Column::new()
            .push(top_row)
            .push(bottom_row)
            .spacing(CONTROL_SPACING)
            .width(Length::Fill),
    )
    .padding(SECTION_PADDING)
    .width(Length::Fill)
    .style(|theme: &Theme| {
        let palette = theme.palette();
        iced::widget::container::Style {
            background: Some(palette.background.scale_alpha(0.85).into()),
            border: iced::Border {
                color: palette.primary.scale_alpha(0.15),
                width: 1.0,
                radius: iced::border::Radius::from(0),
            },
            ..Default::default()
        }
    })
    .into()
}

fn separator<'a>() -> Element<'a, Message> {
    Container::new(Text::new(""))
        .width(Length::Fixed(1.0))
        .height(Length::Fixed(20.0))
        .style(|theme: &Theme| {
            let palette = theme.palette();
            iced::widget::container::Style {
                background: Some(palette.primary.scale_alpha(0.25).into()),
                ..Default::default()
            }
        })
        .into()
}

fn toolbar_button<'a>(label: &'a str, message: Message) -> Button<'a, Message> {
    Button::new(Text::new(label).size(11))
        .on_press(message)
        .padding(Padding::from([5, 8]))
        .style(|theme: &Theme, status: button::Status| {
            let palette = theme.palette();
            let background = match status {
                button::Status::Hovered => palette.primary.scale_alpha(0.15),
                _ => palette.background.scale_alpha(0.5),
            };
            button::Style {
                background: Some(background.into()),
                text_color: palette.text,
                border: iced::Border {
                    color: palette.primary.scale_alpha(0.2),
                    width: 1.0,
                    radius: iced::border::Radius::from(4.0),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            }
        })
}

fn primary_button<'a>(label: &'a str, message: Message) -> Button<'a, Message> {
    Button::new(Text::new(label).size(11))
        .on_press(message)
        .padding(Padding::from([5, 8]))
        .style(|theme: &Theme, status: button::Status| {
            let palette = theme.palette();
            let background = match status {
                button::Status::Hovered => palette.primary.scale_alpha(0.85),
                _ => palette.primary.scale_alpha(0.7),
            };
            button::Style {
                background: Some(background.into()),
                text_color: palette.background,
                border: iced::Border {
                    color: palette.primary.scale_alpha(0.3),
                    width: 1.0,
                    radius: iced::border::Radius::from(4.0),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            }
        })
}

fn danger_button<'a>(label: &'a str, message: Message) -> Button<'a, Message> {
    Button::new(Text::new(label).size(11))
        .on_press(message)
        .padding(Padding::from([5, 8]))
        .style(|_theme: &Theme, status: button::Status| {
            let background = match status {
                button::Status::Hovered => iced::Color::from_rgb(0.9, 0.25, 0.25),
                _ => iced::Color::from_rgb(0.8, 0.2, 0.2),
            };
            button::Style {
                background: Some(background.into()),
                text_color: iced::Color::WHITE,
                border: iced::Border {
                    color: iced::Color::from_rgb(0.9, 0.3, 0.3),
                    width: 1.0,
                    radius: iced::border::Radius::from(4.0),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            }
        })
}

fn success_button<'a>(label: &'a str, message: Message) -> Button<'a, Message> {
    Button::new(Text::new(label).size(11))
        .on_press(message)
        .padding(Padding::from([5, 8]))
        .style(|_theme: &Theme, status: button::Status| {
            let background = match status {
                button::Status::Hovered => iced::Color::from_rgb(0.25, 0.7, 0.35),
                _ => iced::Color::from_rgb(0.2, 0.6, 0.28),
            };
            button::Style {
                background: Some(background.into()),
                text_color: iced::Color::WHITE,
                border: iced::Border {
                    color: iced::Color::from_rgb(0.3, 0.7, 0.4),
                    width: 1.0,
                    radius: iced::border::Radius::from(4.0),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            }
        })
}

fn status_footer<'a>(app: &'a ServerPickerApp) -> Element<'a, Message> {
    let paypal = Button::new(Text::new("PayPal").size(12))
        .on_press(Message::OpenUrl("https://paypal.me".to_string()))
        .padding(4);
    let github = Button::new(Text::new("GitHub").size(12))
        .on_press(Message::OpenUrl("https://github.com".to_string()))
        .padding(4);
    let settings = Button::new(Text::new("Settings").size(12))
        .on_press(Message::StatusSet(
            "Settings window not yet implemented".to_string(),
        ))
        .padding(4);
    let auto_ping = Button::new(
        Text::new(if app.ping_polling_enabled() {
            "Auto-Ping: On"
        } else {
            "Auto-Ping: Off"
        })
        .size(12),
    )
    .on_press(Message::TogglePingPolling)
    .padding(4);

    let status = Container::new(Text::new(&app.status_message).size(12))
        .align_y(iced::Alignment::Center)
        .height(Length::Fill)
        .width(Length::Fill);

    Row::new()
        .push(paypal)
        .push(github)
        .push(settings)
        .push(auto_ping)
        .push(status)
        .spacing(CONTROL_SPACING)
        .padding(SECTION_PADDING)
        .width(Length::Fill)
        .into()
}

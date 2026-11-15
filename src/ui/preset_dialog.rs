use crate::app::{Message, PresetDialogState, ServerPickerApp};
use crate::core::preset::Preset;
use iced::widget::{button, Button, Column, Container, Row, Scrollable, Text, TextInput};
use iced::{Alignment, Element, Length, Padding, Theme};

const DIALOG_WIDTH: f32 = 420.0;
const DIALOG_HEIGHT: f32 = 360.0;
const ROW_SPACING: f32 = 8.0;
const BUTTON_SPACING: f32 = 6.0;

pub fn view<'a>(app: &'a ServerPickerApp) -> Option<Element<'a, Message>> {
    let dialog = app.preset_dialog.as_ref()?;
    let presets = app
        .settings
        .settings()
        .presets_for_game_mode(&app.current_game_mode);

    Some(
        Container::new(
            Column::new()
                .push(dialog_header())
                .push(new_preset_row(dialog))
                .push(preset_list(
                    dialog,
                    presets,
                    app.active_preset_name.as_deref(),
                ))
                .push(close_button())
                .spacing(ROW_SPACING)
                .padding(Padding::from([16, 16])),
        )
        .width(Length::Fixed(DIALOG_WIDTH))
        .height(Length::Fixed(DIALOG_HEIGHT))
        .style(|theme: &Theme| {
            let palette = theme.palette();
            iced::widget::container::Style {
                background: Some(palette.background.into()),
                border: iced::Border {
                    color: palette.text.scale_alpha(0.08),
                    width: 1.0,
                    radius: iced::border::Radius::from(8.0),
                },
                ..Default::default()
            }
        })
        .into(),
    )
}

fn dialog_header<'a>() -> Element<'a, Message> {
    Container::new(Text::new("Manage Presets").size(16))
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .into()
}

fn new_preset_row<'a>(dialog: &'a PresetDialogState) -> Element<'a, Message> {
    let input = TextInput::new("New preset name...", &dialog.new_name)
        .on_input(Message::PresetDialogNameChanged)
        .on_submit(Message::PresetDialogCreate)
        .width(Length::Fill);

    let create = Button::new(Text::new("Create").size(12))
        .on_press(Message::PresetDialogCreate)
        .padding(4);

    Row::new()
        .push(input)
        .push(create)
        .spacing(BUTTON_SPACING)
        .align_y(Alignment::Center)
        .into()
}

fn preset_list<'a>(
    dialog: &'a PresetDialogState,
    presets: Vec<&'a Preset>,
    active_name: Option<&str>,
) -> Element<'a, Message> {
    if presets.is_empty() {
        return Container::new(Text::new("No presets for this game mode.").size(12))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into();
    }

    let rows: Column<'a, Message> = presets.into_iter().fold(
        Column::new().spacing(2).width(Length::Fill),
        |column, preset| column.push(preset_row(dialog, preset, active_name)),
    );

    Scrollable::new(rows)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn preset_row<'a>(
    dialog: &'a PresetDialogState,
    preset: &'a Preset,
    active_name: Option<&str>,
) -> Element<'a, Message> {
    let is_active = active_name == Some(preset.name.as_str());

    let name_area: Element<'a, Message> = if dialog.rename_source.as_deref() == Some(&preset.name) {
        Row::new()
            .push(
                TextInput::new("Rename...", &dialog.rename_value)
                    .on_input(|value| Message::PresetDialogRename {
                        old_name: preset.name.clone(),
                        new_name: value,
                    })
                    .on_submit(Message::PresetDialogRename {
                        old_name: preset.name.clone(),
                        new_name: dialog.rename_value.clone(),
                    })
                    .width(Length::Fill),
            )
            .push(
                Button::new(Text::new("✓").size(12))
                    .on_press(Message::PresetDialogRename {
                        old_name: preset.name.clone(),
                        new_name: dialog.rename_value.clone(),
                    })
                    .padding(4),
            )
            .spacing(BUTTON_SPACING)
            .align_y(Alignment::Center)
            .into()
    } else {
        let label = if is_active {
            format!("{} ●", preset.name)
        } else {
            preset.name.clone()
        };
        Button::new(Text::new(label).size(12))
            .on_press(Message::PresetDialogApply(preset.name.clone()))
            .padding(4)
            .width(Length::Fill)
            .style(|theme: &Theme, status: button::Status| {
                let palette = theme.palette();
                button::Style {
                    background: Some(
                        (if status == button::Status::Hovered {
                            palette.text.scale_alpha(0.06)
                        } else {
                            palette.background.scale_alpha(0.0)
                        })
                        .into(),
                    ),
                    text_color: palette.text,
                    border: iced::Border::default(),
                    shadow: iced::Shadow::default(),
                    snap: false,
                }
            })
            .into()
    };

    let buttons = Row::new()
        .push(
            Button::new(Text::new("Rename").size(11))
                .on_press(Message::PresetDialogRenameStarted(preset.name.clone()))
                .padding(3),
        )
        .push(
            Button::new(Text::new("Dup").size(11))
                .on_press(Message::PresetDialogDuplicate(preset.name.clone()))
                .padding(3),
        )
        .push(delete_button(dialog, &preset.name))
        .spacing(BUTTON_SPACING)
        .align_y(Alignment::Center);

    Row::new()
        .push(name_area)
        .push(buttons)
        .spacing(BUTTON_SPACING)
        .align_y(Alignment::Center)
        .into()
}

fn delete_button<'a>(dialog: &'a PresetDialogState, name: &str) -> Button<'a, Message> {
    if dialog.delete_confirm.as_deref() == Some(name) {
        Button::new(Text::new("Confirm?").size(11))
            .on_press(Message::PresetDialogDelete(name.to_string()))
            .padding(3)
    } else {
        Button::new(Text::new("Del").size(11))
            .on_press(Message::PresetDialogDeleteRequested(name.to_string()))
            .padding(3)
    }
}

fn close_button<'a>() -> Element<'a, Message> {
    Container::new(
        Button::new(Text::new("Close").size(12))
            .on_press(Message::PresetDialogClosed)
            .padding(4),
    )
    .width(Length::Fill)
    .align_x(Alignment::Center)
    .into()
}

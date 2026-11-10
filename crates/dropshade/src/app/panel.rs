//! The terminal's right-click menu and the settings panel. Settings are written into the config
//! file (keeping its comments); the file watcher then applies them like any other edit.

use dropshade_config::edit::{SettingValue, set_value};
use dropshade_config::{CursorShape, TabBarPosition, theme};
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, pick_list, pin, row, slider, stack, text,
    toggler,
};
use iced::{Element, Fill, Font, Length, Point, Task, font};

use super::{App, Message};

const MENU_WIDTH: f32 = 180.0;
const MENU_ITEM_HEIGHT: f32 = 30.0;
const PANEL_WIDTH: f32 = 460.0;
const LABEL_WIDTH: f32 = 150.0;
const TEXT_SIZE: f32 = 13.0;
const BAR_POSITIONS: [&str; 4] = ["top", "bottom", "left", "right"];
const CURSOR_SHAPES: [&str; 3] = ["block", "beam", "underline"];

#[derive(Debug, Clone, Copy)]
pub enum MenuAction {
    Copy,
    Paste,
    NewTab,
    Find,
    Settings,
    Dismiss,
}

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    Theme(String),
    FontSize(f32),
    Opacity(f32),
    Height(f32),
    Width(f32),
    /// A slider was released: write its value.
    Commit(Slider),
    TabBar(String),
    Cursor(String),
    Blink(bool),
    Scrollbar(bool),
    Restore(bool),
    OpenFile,
    Close,
}

#[derive(Debug, Clone, Copy)]
pub enum Slider {
    FontSize,
    Opacity,
    Height,
    Width,
}

/// Slider values while they are being dragged; written to the file on release.
#[derive(Debug, Clone, Copy)]
pub(super) struct SettingsPanel {
    font_size: f32,
    opacity: f32,
    height: f32,
    width: f32,
}

impl App {
    pub(super) fn menu_action(&mut self, action: MenuAction) -> Task<Message> {
        self.terminal_menu = None;
        let shortcut = match action {
            MenuAction::Copy => dropshade_term::Shortcut::Copy,
            MenuAction::Paste => dropshade_term::Shortcut::Paste,
            MenuAction::NewTab => dropshade_term::Shortcut::NewTab,
            MenuAction::Find => dropshade_term::Shortcut::Search,
            MenuAction::Settings => {
                self.open_settings();
                return Task::none();
            }
            MenuAction::Dismiss => return Task::none(),
        };
        self.update(Message::Interaction(dropshade_term::Interaction::Shortcut(
            shortcut,
        )))
    }

    fn open_settings(&mut self) {
        let config = &self.config;
        #[allow(clippy::cast_possible_truncation)]
        let panel = SettingsPanel {
            font_size: config.font.size,
            opacity: config.appearance.background_opacity,
            height: config.window.height as f32,
            width: config.window.width as f32,
        };
        self.settings_panel = Some(panel);
    }

    pub(super) fn settings_message(&mut self, message: SettingsMessage) -> Task<Message> {
        let Some(panel) = self.settings_panel.as_mut() else {
            return Task::none();
        };
        match message {
            SettingsMessage::FontSize(value) => panel.font_size = value,
            SettingsMessage::Opacity(value) => panel.opacity = value,
            SettingsMessage::Height(value) => panel.height = value,
            SettingsMessage::Width(value) => panel.width = value,
            SettingsMessage::Commit(slider) => {
                let panel = *panel;
                let (section, key, value) = match slider {
                    Slider::FontSize => ("font", "size", panel.font_size),
                    Slider::Opacity => ("appearance", "background_opacity", panel.opacity),
                    Slider::Height => ("window", "height", panel.height),
                    Slider::Width => ("window", "width", panel.width),
                };
                self.write_setting(section, key, SettingValue::Float(f64::from(value)));
            }
            SettingsMessage::Theme(name) => {
                self.write_setting("appearance", "theme", SettingValue::Text(name));
            }
            SettingsMessage::TabBar(position) => {
                self.write_setting("appearance", "tab_bar", SettingValue::Text(position));
            }
            SettingsMessage::Cursor(shape) => {
                self.write_setting("appearance", "cursor_shape", SettingValue::Text(shape));
            }
            SettingsMessage::Blink(on) => {
                self.write_setting("appearance", "cursor_blink", SettingValue::Bool(on));
            }
            SettingsMessage::Scrollbar(on) => {
                self.write_setting("appearance", "scrollbar", SettingValue::Bool(on));
            }
            SettingsMessage::Restore(on) => {
                self.write_setting("session", "restore", SettingValue::Bool(on));
            }
            SettingsMessage::OpenFile => self.open_config_file(),
            SettingsMessage::Close => self.settings_panel = None,
        }
        Task::none()
    }

    fn write_setting(&mut self, section: &str, key: &str, value: SettingValue) {
        let Some(path) = self.config_path.clone() else {
            self.config_error = Some("No configuration file path (HOME is not set)".into());
            return;
        };
        if let Err(error) = set_value(&path, section, key, value) {
            tracing::warn!(%error, "could not save the setting");
            self.config_error = Some(error.to_string());
        }
    }

    fn open_config_file(&mut self) {
        let Some(path) = self.config_path.clone() else {
            return;
        };
        if !path.exists()
            && let Err(error) = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, dropshade_config::DEFAULT_CONFIG))
        {
            self.config_error = Some(format!("Cannot create {}: {error}", path.display()));
            return;
        }
        if let Err(error) = open::that_detached(&path) {
            self.config_error = Some(format!("Cannot open {}: {error}", path.display()));
        }
    }

    pub(super) fn terminal_menu_layer(&self, position: Point) -> Element<'_, Message> {
        let colors = self.settings.colors;
        let has_selection = self
            .tabs
            .active()
            .is_some_and(|tab| tab.content.terminal.selection_text().is_some());
        let item = |label: &'static str, action: Option<MenuAction>| {
            let color = if action.is_some() {
                colors.text
            } else {
                colors.muted
            };
            button(text(label).size(TEXT_SIZE))
                .width(Fill)
                .height(MENU_ITEM_HEIGHT)
                .padding([7, 10])
                .style(move |_, status| colors.ghost_button(color, status))
                .on_press_maybe(action.map(Message::Menu))
        };
        let items = column![
            item("Copy", has_selection.then_some(MenuAction::Copy)),
            item("Paste", Some(MenuAction::Paste)),
            item("New Tab", Some(MenuAction::NewTab)),
            item("Find…", Some(MenuAction::Find)),
            item("Settings…", Some(MenuAction::Settings)),
        ];
        #[allow(clippy::cast_precision_loss)]
        let height = 5.0 * MENU_ITEM_HEIGHT + 8.0;
        let anchor = Point::new(
            position
                .x
                .clamp(0.0, (self.window_size.width - MENU_WIDTH).max(0.0)),
            position
                .y
                .clamp(0.0, (self.window_size.height - height).max(0.0)),
        );
        let card = container(items)
            .padding(4)
            .width(MENU_WIDTH)
            .style(move |_| colors.surface_style());
        let dismiss = mouse_area(Space::new().width(Fill).height(Fill))
            .on_press(Message::Menu(MenuAction::Dismiss))
            .on_right_press(Message::Menu(MenuAction::Dismiss));
        stack![dismiss, pin(card).position(anchor)].into()
    }

    pub(super) fn settings_layer(&self, panel: SettingsPanel) -> Element<'_, Message> {
        let colors = self.settings.colors;
        let config = &self.config;
        let current_theme = config.appearance.theme.name(self.prefers_light).to_owned();
        let bar = bar_name(config.appearance.tab_bar);
        let cursor = cursor_name(config.appearance.cursor_shape);
        let choices = |items: &[&str]| items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let bold = Font {
            weight: font::Weight::Bold,
            ..Font::DEFAULT
        };
        let content = column![
            text("Settings").size(16).font(bold),
            setting(
                "Theme",
                pick_list(theme_names(), Some(current_theme), |name| {
                    Message::Settings(SettingsMessage::Theme(name))
                })
                .text_size(TEXT_SIZE)
            ),
            slider_setting(
                "Font size",
                8.0..=32.0,
                panel.font_size,
                1.0,
                SettingsMessage::FontSize,
                Slider::FontSize,
                format!("{:.0}", panel.font_size),
                colors
            ),
            slider_setting(
                "Background opacity",
                0.3..=1.0,
                panel.opacity,
                0.01,
                SettingsMessage::Opacity,
                Slider::Opacity,
                percent(panel.opacity),
                colors
            ),
            setting(
                "Tab bar",
                pick_list(choices(&BAR_POSITIONS), Some(bar.to_owned()), |p| {
                    Message::Settings(SettingsMessage::TabBar(p))
                })
                .text_size(TEXT_SIZE)
            ),
            slider_setting(
                "Drop-down height",
                0.1..=1.0,
                panel.height,
                0.05,
                SettingsMessage::Height,
                Slider::Height,
                percent(panel.height),
                colors
            ),
            slider_setting(
                "Drop-down width",
                0.2..=1.0,
                panel.width,
                0.05,
                SettingsMessage::Width,
                Slider::Width,
                percent(panel.width),
                colors
            ),
            cursor_setting(cursor, config.appearance.cursor_blink),
            setting(
                "Scrollbar",
                toggler(config.appearance.scrollbar)
                    .on_toggle(|on| Message::Settings(SettingsMessage::Scrollbar(on)))
            ),
            setting(
                "Reopen tabs on start",
                toggler(config.session.restore)
                    .on_toggle(|on| Message::Settings(SettingsMessage::Restore(on)))
            ),
            text("Changes are saved to config.toml right away.")
                .size(12)
                .color(colors.muted),
            panel_actions(colors),
        ]
        .spacing(12);
        let card = container(content)
            .padding(20)
            .width(PANEL_WIDTH)
            .style(move |_| colors.surface_style());
        opaque(
            container(card)
                .center(Fill)
                .style(move |_| colors.backdrop_style()),
        )
    }
}

fn setting<'a>(
    name: &'static str,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        text(name).size(TEXT_SIZE).width(Length::Fixed(LABEL_WIDTH)),
        control.into()
    ]
    .spacing(10)
    .align_y(iced::Center)
    .into()
}

#[allow(clippy::too_many_arguments)]
fn slider_setting(
    name: &'static str,
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    step: f32,
    on_change: fn(f32) -> SettingsMessage,
    which: Slider,
    shown: String,
    colors: crate::theme::UiColors,
) -> Element<'static, Message> {
    setting(
        name,
        row![
            slider(range, value, move |v| Message::Settings(on_change(v)))
                .step(step)
                .on_release(Message::Settings(SettingsMessage::Commit(which))),
            text(shown)
                .size(TEXT_SIZE)
                .color(colors.muted)
                .width(Length::Fixed(48.0)),
        ]
        .spacing(8),
    )
}

fn cursor_setting(cursor: &str, blink: bool) -> Element<'static, Message> {
    let choices = CURSOR_SHAPES
        .iter()
        .map(|s| (*s).to_owned())
        .collect::<Vec<_>>();
    setting(
        "Cursor",
        row![
            pick_list(choices, Some(cursor.to_owned()), |c| Message::Settings(
                SettingsMessage::Cursor(c)
            ))
            .text_size(TEXT_SIZE),
            toggler(blink)
                .label("Blink")
                .text_size(TEXT_SIZE)
                .on_toggle(|on| Message::Settings(SettingsMessage::Blink(on))),
        ]
        .spacing(12)
        .align_y(iced::Center),
    )
}

fn panel_actions(colors: crate::theme::UiColors) -> Element<'static, Message> {
    row![
        Space::new().width(Fill),
        button(text("Open config file").size(TEXT_SIZE))
            .padding([7, 14])
            .style(move |_, status| colors.ghost_button(colors.text, status))
            .on_press(Message::Settings(SettingsMessage::OpenFile)),
        button(text("Done").size(TEXT_SIZE))
            .padding([7, 14])
            .style(move |_, status| colors.accent_button(status))
            .on_press(Message::Settings(SettingsMessage::Close)),
    ]
    .spacing(8)
    .into()
}

fn bar_name(position: TabBarPosition) -> &'static str {
    match position {
        TabBarPosition::Top => "top",
        TabBarPosition::Bottom => "bottom",
        TabBarPosition::Left => "left",
        TabBarPosition::Right => "right",
    }
}

fn cursor_name(shape: CursorShape) -> &'static str {
    match shape {
        CursorShape::Block => "block",
        CursorShape::Beam => "beam",
        CursorShape::Underline => "underline",
    }
}

fn percent(value: f32) -> String {
    format!("{:.0}%", value * 100.0)
}

/// Built-in themes followed by the theme files in the themes directory.
fn theme_names() -> Vec<String> {
    let mut names: Vec<String> = theme::BUILT_IN
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    if let Some(dir) = dropshade_config::themes_dir()
        && let Ok(entries) = std::fs::read_dir(dir)
    {
        let mut files: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
            .filter_map(|path| {
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
            })
            .filter(|name| !names.contains(name))
            .collect();
        files.sort();
        names.extend(files);
    }
    names
}

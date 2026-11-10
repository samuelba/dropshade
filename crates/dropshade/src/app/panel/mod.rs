//! The settings panel. Every change is written into the config file right away (keeping its
//! comments); the file watcher then applies it like any other edit.

mod fields;
mod view;

use std::collections::{HashMap, HashSet};
use std::fmt;

use dropshade_config::ThemeChoice;
use dropshade_config::edit::{SettingValue, remove_value, set_value};
use dropshade_term::input::Chord;
use iced::Task;

use super::{App, Message};
use crate::screens::{self, Screen, ScreenChoice};
use fields::{Field, Slider, Switch};

const DEFAULT_LIGHT_THEME: &str = "dawn";
const DEFAULT_DARK_THEME: &str = "dusk";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Appearance,
    Font,
    Window,
    Terminal,
    Input,
    Session,
    Keybindings,
}

impl Section {
    pub const ALL: [Self; 7] = [
        Self::Appearance,
        Self::Font,
        Self::Window,
        Self::Terminal,
        Self::Input,
        Self::Session,
        Self::Keybindings,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Font => "Font",
            Self::Window => "Window",
            Self::Terminal => "Terminal",
            Self::Input => "Input",
            Self::Session => "Session",
            Self::Keybindings => "Keybindings",
        }
    }
}

/// A pick list whose choice is written as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Theme,
    DarkTheme,
    LightTheme,
    TabBar,
    Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BindingKind {
    #[default]
    Action,
    Send,
}

impl fmt::Display for BindingKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Action => "runs",
            Self::Send => "sends text",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutChoice {
    None,
    Named(String),
}

impl fmt::Display for LayoutChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => f.write_str("None"),
            Self::Named(name) => f.write_str(name),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    Section(Section),
    Slide(Slider, f32),
    /// A slider was released: write its value.
    Release(Slider),
    Edit(Field, String),
    /// Enter was pressed in a text field.
    Submit(Field),
    Choose(Choice, String),
    Toggle(Switch, bool),
    Screen(ScreenChoice),
    /// The connected screens arrived from the compositor (`None` if it could not be asked).
    Screens(Option<Vec<Screen>>),
    Layout(LayoutChoice),
    AddEnv,
    RemoveEnv(String),
    BindingKind(BindingKind),
    BindingAction(String),
    AddBinding,
    RemoveBinding(String),
    OpenFile,
    Close,
}

/// Values being edited (sliders while dragged, text until Enter), and what the panel lists.
#[derive(Debug, Clone, Default)]
pub(super) struct SettingsPanel {
    section: Section,
    sliders: HashMap<Slider, f32>,
    drafts: HashMap<Field, String>,
    unsaved: HashSet<Field>,
    binding_kind: BindingKind,
    binding_action: String,
    /// Unknown until the compositor answers.
    screens: Option<Vec<Screen>>,
    layouts: Vec<String>,
    /// Why the last change could not be saved.
    error: Option<String>,
}

impl SettingsPanel {
    fn slider(&self, slider: Slider, config: &dropshade_config::Config) -> f32 {
        self.sliders
            .get(&slider)
            .copied()
            .unwrap_or_else(|| slider.current(config))
    }

    fn text(&self, field: Field, config: &dropshade_config::Config) -> String {
        self.drafts
            .get(&field)
            .cloned()
            .unwrap_or_else(|| field.current(config))
    }

    fn take_draft(&mut self, field: Field) -> String {
        self.drafts.remove(&field).unwrap_or_default()
    }
}

impl App {
    pub(super) fn open_settings(&mut self) -> Task<Message> {
        self.settings_panel = Some(SettingsPanel {
            layouts: dropshade_session::layouts_dir()
                .map(|dir| dropshade_session::list_layouts(&dir))
                .unwrap_or_default(),
            binding_action: fields::action_names().remove(0),
            ..SettingsPanel::default()
        });
        let connection = self.ipc.clone();
        Task::perform(
            async move {
                let connection = match connection {
                    Some(connection) => connection,
                    None => zbus::Connection::session().await?,
                };
                screens::connected(&connection).await
            },
            |result| {
                let screens = result
                    .inspect_err(|error| tracing::warn!(%error, "cannot list the screens"))
                    .ok();
                Message::Settings(SettingsMessage::Screens(screens))
            },
        )
    }

    pub(super) fn settings_message(&mut self, message: SettingsMessage) -> Task<Message> {
        let Some(mut panel) = self.settings_panel.take() else {
            return Task::none();
        };
        match self.apply_setting(&mut panel, message) {
            Ok(false) => return Task::none(),
            Ok(true) => panel.error = None,
            Err(error) => panel.error = Some(error),
        }
        self.settings_panel = Some(panel);
        Task::none()
    }

    /// Handles one panel message; `Ok(false)` closes the panel.
    fn apply_setting(
        &mut self,
        panel: &mut SettingsPanel,
        message: SettingsMessage,
    ) -> Result<bool, String> {
        match message {
            SettingsMessage::Section(section) => panel.section = section,
            SettingsMessage::Slide(slider, value) => {
                panel.sliders.insert(slider, value);
            }
            SettingsMessage::Release(slider) => {
                let spec = slider.spec();
                let value = slider.value(panel.slider(slider, &self.config));
                self.write(spec.section, spec.key, value)?;
            }
            SettingsMessage::Edit(field, text) => {
                panel.drafts.insert(field, text);
                if field.key().is_some() {
                    panel.unsaved.insert(field);
                }
            }
            SettingsMessage::Submit(Field::EnvName | Field::EnvValue) | SettingsMessage::AddEnv => {
                self.add_env(panel)?;
            }
            SettingsMessage::Submit(Field::Chord | Field::SendText)
            | SettingsMessage::AddBinding => self.add_binding(panel)?,
            SettingsMessage::Submit(field) => self.save_field(panel, field)?,
            SettingsMessage::Choose(choice, name) => self.choose(choice, name)?,
            SettingsMessage::Toggle(switch, on) => self.toggle(panel, switch, on)?,
            SettingsMessage::Screen(choice) => {
                let value = SettingValue::Text(choice.monitor.as_str().to_owned());
                self.write("window", "monitor", value)?;
            }
            SettingsMessage::Screens(screens) => panel.screens = screens,
            SettingsMessage::Layout(choice) => {
                let name = match choice {
                    LayoutChoice::None => String::new(),
                    LayoutChoice::Named(name) => name,
                };
                self.write("session", "layout", SettingValue::Text(name))?;
            }
            SettingsMessage::RemoveEnv(name) => self.remove("terminal.env", &name)?,
            SettingsMessage::BindingKind(kind) => panel.binding_kind = kind,
            SettingsMessage::BindingAction(action) => panel.binding_action = action,
            SettingsMessage::RemoveBinding(chord) => self.remove("keybindings", &chord)?,
            SettingsMessage::OpenFile => self.open_config_file()?,
            SettingsMessage::Close => {
                self.save_unsaved(panel)?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn save_field(&self, panel: &mut SettingsPanel, field: Field) -> Result<(), String> {
        let Some((section, key)) = field.key() else {
            return Ok(());
        };
        let value = field.parse(&panel.text(field, &self.config))?;
        self.write(section, key, value)?;
        panel.unsaved.remove(&field);
        Ok(())
    }

    /// Saves text fields that were edited without pressing Enter. On failure the panel shows the
    /// page with the offending field.
    fn save_unsaved(&self, panel: &mut SettingsPanel) -> Result<(), String> {
        let edited: Vec<Field> = Field::SETTINGS
            .into_iter()
            .filter(|field| panel.unsaved.contains(field))
            .collect();
        for field in edited {
            if let Err(error) = self.save_field(panel, field) {
                panel.section = section_of(field);
                return Err(error);
            }
        }
        Ok(())
    }

    fn choose(&self, choice: Choice, name: String) -> Result<(), String> {
        let (dark, light) = match &self.config.appearance.theme {
            ThemeChoice::FollowSystem { dark, light } => (dark.clone(), light.clone()),
            ThemeChoice::Single(theme) => (theme.clone(), theme.clone()),
        };
        match choice {
            Choice::Theme => self.write("appearance", "theme", SettingValue::Text(name)),
            Choice::DarkTheme => self.write("appearance", "theme", theme_pair(name, light)),
            Choice::LightTheme => self.write("appearance", "theme", theme_pair(dark, name)),
            Choice::TabBar => self.write("appearance", "tab_bar", SettingValue::Text(name)),
            Choice::Cursor => self.write("appearance", "cursor_shape", SettingValue::Text(name)),
        }
    }

    fn toggle(&self, panel: &mut SettingsPanel, switch: Switch, on: bool) -> Result<(), String> {
        if let Some((section, key)) = switch.key() {
            return self.write(section, key, SettingValue::Bool(on));
        }
        match switch {
            Switch::FollowSystem if on => {
                let current = self.config.appearance.theme.name(self.prefers_light);
                let pair = if current == DEFAULT_LIGHT_THEME {
                    theme_pair(DEFAULT_DARK_THEME.into(), current.into())
                } else {
                    theme_pair(current.into(), DEFAULT_LIGHT_THEME.into())
                };
                self.write("appearance", "theme", pair)
            }
            Switch::FollowSystem => {
                let current = self.config.appearance.theme.name(self.prefers_light);
                self.write("appearance", "theme", SettingValue::Text(current.into()))
            }
            Switch::UnlimitedScrollback => {
                panel.drafts.remove(&Field::Scrollback);
                panel.unsaved.remove(&Field::Scrollback);
                if on {
                    let value = SettingValue::Text("unlimited".into());
                    self.write("terminal", "scrollback", value)
                } else {
                    self.remove("terminal", "scrollback")
                }
            }
            _ => Ok(()),
        }
    }

    fn add_env(&self, panel: &mut SettingsPanel) -> Result<(), String> {
        let name = panel.text(Field::EnvName, &self.config).trim().to_owned();
        let valid = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && name.chars().next().is_some_and(|c| !c.is_ascii_digit());
        if !valid {
            return Err(format!(
                "\"{name}\" is not a variable name (letters, digits and _)"
            ));
        }
        let value = panel.text(Field::EnvValue, &self.config);
        self.write("terminal.env", &name, SettingValue::Text(value))?;
        panel.take_draft(Field::EnvName);
        panel.take_draft(Field::EnvValue);
        Ok(())
    }

    fn add_binding(&self, panel: &mut SettingsPanel) -> Result<(), String> {
        let chord = panel
            .text(Field::Chord, &self.config)
            .trim()
            .to_ascii_lowercase();
        Chord::parse(&chord)?;
        let value = match panel.binding_kind {
            BindingKind::Action => SettingValue::Text(panel.binding_action.clone()),
            BindingKind::Send => {
                let SettingValue::Text(send) =
                    Field::SendText.parse(&panel.text(Field::SendText, &self.config))?
                else {
                    return Err("the text to send is invalid".into());
                };
                SettingValue::Pairs(vec![("send".into(), send)])
            }
        };
        self.write("keybindings", &chord, value)?;
        panel.take_draft(Field::Chord);
        panel.take_draft(Field::SendText);
        Ok(())
    }

    fn write(&self, section: &str, key: &str, value: SettingValue) -> Result<(), String> {
        let path = self.writable_config()?;
        set_value(path, section, key, value).map_err(|error| {
            tracing::warn!(%error, "could not save the setting");
            error.to_string()
        })
    }

    fn remove(&self, section: &str, key: &str) -> Result<(), String> {
        let path = self.writable_config()?;
        remove_value(path, section, key).map_err(|error| {
            tracing::warn!(%error, "could not remove the setting");
            error.to_string()
        })
    }

    fn writable_config(&self) -> Result<&std::path::Path, String> {
        self.config_path
            .as_deref()
            .ok_or_else(|| "No configuration file path (HOME is not set)".into())
    }

    fn open_config_file(&self) -> Result<(), String> {
        let path = self.writable_config()?;
        if !path.exists() {
            path.parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(path, dropshade_config::DEFAULT_CONFIG))
                .map_err(|error| format!("Cannot create {}: {error}", path.display()))?;
        }
        open::that_detached(path)
            .map_err(|error| format!("Cannot open {}: {error}", path.display()))
    }
}

fn section_of(field: Field) -> Section {
    match field {
        Field::FontFamily => Section::Font,
        Field::Hotkey => Section::Window,
        Field::ShiftEnter => Section::Input,
        Field::Chord | Field::SendText => Section::Keybindings,
        Field::Shell
        | Field::ShellArgs
        | Field::WorkingDirectory
        | Field::Scrollback
        | Field::EnvName
        | Field::EnvValue => Section::Terminal,
    }
}

fn theme_pair(dark: String, light: String) -> SettingValue {
    SettingValue::Pairs(vec![("dark".into(), dark), ("light".into(), light)])
}

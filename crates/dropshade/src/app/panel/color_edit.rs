//! What the Colors page's messages do: move the picker's draft and write or reset colors.

use dropshade_config::edit::{SettingValue, remove_table, remove_value_pruned};
use dropshade_config::theme::Rgb8;

use super::hsv::Hsv;
use super::{PickerState, SettingsMessage, SettingsPanel};
use crate::app::App;

impl App {
    pub(super) fn edit_color(
        &self,
        panel: &mut SettingsPanel,
        message: SettingsMessage,
    ) -> Result<(), String> {
        match message {
            SettingsMessage::OpenPicker(slot) => {
                let already_open = panel.picker.as_ref().is_some_and(|open| open.slot == slot);
                self.save_picker(panel)?;
                panel.picker = (!already_open).then(|| PickerState::new(slot));
            }
            SettingsMessage::PickerDrag(hsv) => {
                if let Some(picker) = panel.picker.as_mut() {
                    picker.draft = Some(hsv);
                    picker.hex = None;
                    picker.unsaved = true;
                }
            }
            SettingsMessage::PickerRelease => self.save_picker(panel)?,
            SettingsMessage::PickerHex(text) => {
                if let Some(picker) = panel.picker.as_mut() {
                    match Rgb8::parse(&text) {
                        Some(color) => {
                            picker.draft = Some(Hsv::from_rgb(color));
                            picker.unsaved = true;
                        }
                        None if picker.unsaved => {
                            picker.draft = None;
                            picker.unsaved = false;
                        }
                        None => {}
                    }
                    picker.hex = Some(text);
                }
            }
            SettingsMessage::PickerHexSubmit => {
                if let Some(typed) = panel.picker.as_ref().and_then(|picker| picker.hex.as_ref())
                    && Rgb8::parse(typed).is_none()
                {
                    return Err(format!("\"{}\" is not a color like #1e1e2e", typed.trim()));
                }
                self.save_picker(panel)?;
                if let Some(picker) = panel.picker.as_mut() {
                    picker.hex = None;
                }
            }
            SettingsMessage::QuickPick(color) => {
                if let Some(picker) = panel.picker.as_mut() {
                    picker.draft = Some(Hsv::from_rgb(color));
                    picker.hex = None;
                    picker.unsaved = true;
                }
                self.save_picker(panel)?;
            }
            SettingsMessage::ResetColor(slot) => {
                let (section, key) = slot.key();
                remove_value_pruned(self.writable_config()?, section, key)
                    .map_err(|error| reset_failed(&error))?;
                if let Some(picker) = panel.picker.as_mut().filter(|open| open.slot == slot) {
                    picker.discard();
                }
            }
            SettingsMessage::ResetAllColors if !panel.confirm_reset_colors => {
                panel.confirm_reset_colors = true;
            }
            SettingsMessage::ResetAllColors => {
                panel.confirm_reset_colors = false;
                remove_table(self.writable_config()?, "colors")
                    .map_err(|error| reset_failed(&error))?;
                if let Some(picker) = panel.picker.as_mut() {
                    picker.discard();
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Writes the picker's color if it changed since it was last written.
    pub(super) fn save_picker(&self, panel: &mut SettingsPanel) -> Result<(), String> {
        let Some(picker) = panel.picker.as_mut().filter(|picker| picker.unsaved) else {
            return Ok(());
        };
        let Some(hsv) = picker.draft else {
            return Ok(());
        };
        let (section, key) = picker.slot.key();
        self.write(section, key, SettingValue::Text(hsv.to_rgb().to_hex()))?;
        picker.unsaved = false;
        Ok(())
    }
}

fn reset_failed(error: &dropshade_config::ConfigError) -> String {
    tracing::warn!(%error, "could not reset the color");
    error.to_string()
}

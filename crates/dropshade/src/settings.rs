//! Turns the configuration file into everything the app needs at runtime. Problems that do not
//! prevent starting (unknown theme, missing font, bad keybinding) become warnings and fall back
//! to defaults.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use dropshade_config::theme::{self, Rgb8, ThemeColors};
use dropshade_config::{Binding as ConfigBinding, Config, CursorShape as ConfigCursorShape};
use dropshade_term::alacritty_terminal::vte::ansi::Rgb;
use dropshade_term::input::{Binding, BindingAction, Chord};
use dropshade_term::{CursorShape, CursorStyle, InputConfig, Palette, ViewStyle, metrics};
use iced::Font;

use crate::tabs::BarPosition;
use crate::theme::UiColors;

const DEFAULT_FONT_FAMILY: &str = "monospace";

#[derive(Debug, Clone)]
pub struct Settings {
    pub style: ViewStyle,
    pub colors: UiColors,
    pub input: InputConfig,
    pub bar: BarPosition,
    pub shell: Option<(String, Vec<String>)>,
    pub env: HashMap<String, String>,
    pub working_directory: Option<PathBuf>,
    pub scrollback_lines: usize,
    pub cursor_style: CursorStyle,
    pub copy_on_select: bool,
    pub warnings: Vec<String>,
}

/// Builds runtime settings. `prefers_light` picks the light theme of a light/dark pair.
pub fn build(config: &Config, prefers_light: bool, themes_dir: Option<&Path>) -> Settings {
    let mut warnings = Vec::new();
    let theme_name = config.appearance.theme.name(prefers_light);
    let palette = theme::resolve(theme_name, themes_dir).map_or_else(
        |error| {
            warnings.push(format!("{error}; using \"dusk\""));
            Palette::DUSK
        },
        |colors| palette(&colors),
    );
    let font = font(&config.font.family, &mut warnings);
    let style = ViewStyle {
        palette,
        font,
        font_size: config.font.size,
        cell: metrics::cell_size(font, config.font.size, config.font.line_height),
        padding: config.appearance.padding,
        background_opacity: config.appearance.background_opacity,
        scrollbar: config.appearance.scrollbar,
    };
    let input = InputConfig {
        shift_enter: Some(config.input.shift_enter.as_bytes().to_vec())
            .filter(|bytes| !bytes.is_empty()),
        ime: config.input.ime,
        bindings: bindings(config, &mut warnings),
    };
    let terminal = &config.terminal;
    Settings {
        colors: UiColors::from_palette(&palette),
        style,
        input,
        bar: bar_position(config.appearance.tab_bar),
        shell: (!terminal.shell.trim().is_empty())
            .then(|| (terminal.shell.clone(), terminal.shell_args.clone())),
        env: terminal.env.clone().into_iter().collect(),
        working_directory: expand_home(&terminal.working_directory),
        scrollback_lines: terminal.scrollback.lines(),
        cursor_style: CursorStyle {
            shape: cursor_shape(config.appearance.cursor_shape),
            blinking: config.appearance.cursor_blink,
        },
        copy_on_select: config.input.copy_on_select,
        warnings,
    }
}

impl Settings {
    /// These settings with the terminal font scaled by `scale`.
    #[must_use]
    pub fn zoomed(mut self, scale: f32, line_height: f32) -> Self {
        let size = self.style.font_size * scale;
        self.style.font_size = size;
        self.style.cell = metrics::cell_size(self.style.font, size, line_height);
        self
    }
}

fn rgb(color: Rgb8) -> Rgb {
    Rgb {
        r: color.r,
        g: color.g,
        b: color.b,
    }
}

fn palette(colors: &ThemeColors) -> Palette {
    Palette {
        foreground: rgb(colors.foreground),
        background: rgb(colors.background),
        cursor: rgb(colors.cursor),
        normal: colors.normal.map(rgb),
        bright: colors.bright.map(rgb),
    }
}

fn font(family: &str, warnings: &mut Vec<String>) -> Font {
    let family = family.trim();
    if family.eq_ignore_ascii_case(DEFAULT_FONT_FAMILY) {
        return Font::MONOSPACE;
    }
    if !metrics::font_installed(family) {
        warnings.push(format!(
            "font \"{family}\" is not installed; using the default monospace font"
        ));
        return Font::MONOSPACE;
    }
    Font::with_name(intern(family))
}

/// iced fonts need `'static` names; each distinct family is leaked once and reused afterwards.
fn intern(name: &str) -> &'static str {
    static NAMES: OnceLock<Mutex<Vec<&'static str>>> = OnceLock::new();
    let names = NAMES.get_or_init(|| Mutex::new(Vec::new()));
    let Ok(mut names) = names.lock() else {
        return Box::leak(name.to_owned().into_boxed_str());
    };
    if let Some(existing) = names.iter().find(|existing| **existing == name) {
        return existing;
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.push(leaked);
    leaked
}

fn bindings(config: &Config, warnings: &mut Vec<String>) -> Vec<Binding> {
    config
        .keybindings
        .iter()
        .filter_map(|(chord, binding)| match binding_action(binding) {
            Ok(action) => match Chord::parse(chord) {
                Ok(chord) => Some(Binding { chord, action }),
                Err(error) => {
                    warnings.push(format!("keybinding \"{chord}\": {error}"));
                    None
                }
            },
            Err(error) => {
                warnings.push(format!("keybinding \"{chord}\": {error}"));
                None
            }
        })
        .collect()
}

fn binding_action(binding: &ConfigBinding) -> Result<BindingAction, String> {
    match binding {
        ConfigBinding::Send { send } => Ok(BindingAction::Send(send.as_bytes().to_vec())),
        ConfigBinding::Action(name) if name.trim().eq_ignore_ascii_case("none") => {
            Ok(BindingAction::PassThrough)
        }
        ConfigBinding::Action(name) => name.parse().map(BindingAction::Shortcut),
    }
}

fn bar_position(position: dropshade_config::TabBarPosition) -> BarPosition {
    use dropshade_config::TabBarPosition as Config;
    match position {
        Config::Top => BarPosition::Top,
        Config::Bottom => BarPosition::Bottom,
        Config::Left => BarPosition::Left,
        Config::Right => BarPosition::Right,
    }
}

fn cursor_shape(shape: ConfigCursorShape) -> CursorShape {
    match shape {
        ConfigCursorShape::Block => CursorShape::Block,
        ConfigCursorShape::Beam => CursorShape::Beam,
        ConfigCursorShape::Underline => CursorShape::Underline,
    }
}

fn expand_home(path: &str) -> Option<PathBuf> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    match path.strip_prefix('~') {
        Some(rest) => {
            let home = std::env::var_os("HOME").map(PathBuf::from)?;
            Some(home.join(rest.trim_start_matches('/')))
        }
        None => Some(PathBuf::from(path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dropshade_config::parse;

    fn settings(text: &str) -> Settings {
        let config = parse(text, Path::new("test.toml")).expect("valid config");
        build(&config, false, None)
    }

    #[test]
    fn defaults_produce_no_warnings() {
        let settings = settings("");
        assert!(settings.warnings.is_empty(), "{:?}", settings.warnings);
        assert_eq!(settings.style.palette, Palette::DUSK);
        assert_eq!(settings.bar, BarPosition::Top);
        assert_eq!(settings.shell, None);
        assert_eq!(settings.input.shift_enter.as_deref(), Some(&b"\x1b\r"[..]));
        assert_eq!(settings.scrollback_lines, 10_000);
    }

    #[test]
    fn zooming_scales_the_font_and_the_cells() {
        let normal = settings("");
        let zoomed = normal.clone().zoomed(2.0, 1.25);
        assert!((zoomed.style.font_size - 2.0 * normal.style.font_size).abs() < f32::EPSILON);
        assert!(zoomed.style.cell.width > normal.style.cell.width);
        assert!(zoomed.style.cell.height > normal.style.cell.height);
    }

    #[test]
    fn appearance_values_are_applied() {
        let settings = settings(
            "[appearance]\ntheme = \"dawn\"\npadding = 4\nbackground_opacity = 0.5\ntab_bar = \"bottom\"\n\
             cursor_shape = \"beam\"\ncursor_blink = true\n[font]\nsize = 20\n",
        );
        assert_eq!(
            settings.style.palette.background,
            rgb(theme::DAWN.background)
        );
        assert!((settings.style.padding - 4.0).abs() < f32::EPSILON);
        assert!((settings.style.background_opacity - 0.5).abs() < f32::EPSILON);
        assert!((settings.style.font_size - 20.0).abs() < f32::EPSILON);
        assert_eq!(settings.bar, BarPosition::Bottom);
        assert_eq!(
            settings.cursor_style,
            CursorStyle {
                shape: CursorShape::Beam,
                blinking: true
            }
        );
    }

    #[test]
    fn follow_system_themes_pick_by_preference() {
        let config = parse(
            "[appearance]\ntheme = { dark = \"dusk\", light = \"dawn\" }",
            Path::new("t"),
        )
        .expect("valid");
        assert_eq!(
            build(&config, true, None).style.palette.background,
            rgb(theme::DAWN.background)
        );
        assert_eq!(build(&config, false, None).style.palette, Palette::DUSK);
    }

    #[test]
    fn unknown_themes_and_fonts_fall_back_with_a_warning() {
        let settings =
            settings("[appearance]\ntheme = \"nope\"\n[font]\nfamily = \"Not A Font 42\"\n");
        assert_eq!(settings.style.palette, Palette::DUSK);
        assert_eq!(settings.style.font, Font::MONOSPACE);
        assert_eq!(settings.warnings.len(), 2, "{:?}", settings.warnings);
        assert!(settings.warnings.iter().any(|w| w.contains("nope")));
        assert!(
            settings
                .warnings
                .iter()
                .any(|w| w.contains("Not A Font 42"))
        );
    }

    #[test]
    fn keybindings_are_converted_and_bad_ones_reported() {
        let settings = settings(
            "[keybindings]\n\"ctrl+alt+l\" = { send = \"clear\\r\" }\n\"ctrl+shift+n\" = \"new-tab\"\n\
             \"ctrl+tab\" = \"none\"\n\"ctrl+banana\" = \"copy\"\n\"ctrl+k\" = \"explode\"\n",
        );
        let actions: Vec<&BindingAction> =
            settings.input.bindings.iter().map(|b| &b.action).collect();
        assert_eq!(actions.len(), 3);
        assert!(actions.contains(&&BindingAction::Send(b"clear\r".to_vec())));
        assert!(actions.contains(&&BindingAction::PassThrough));
        assert_eq!(settings.warnings.len(), 2, "{:?}", settings.warnings);
    }

    #[test]
    fn shell_and_environment_are_passed_on() {
        let settings = settings(
            "[terminal]\nshell = \"/bin/zsh\"\nshell_args = [\"-l\"]\nscrollback = \"unlimited\"\n\
             [terminal.env]\nEDITOR = \"nvim\"\n[input]\nshift_enter = \"\"\ncopy_on_select = true\n",
        );
        assert_eq!(settings.shell, Some(("/bin/zsh".into(), vec!["-l".into()])));
        assert_eq!(settings.env.get("EDITOR").map(String::as_str), Some("nvim"));
        assert_eq!(settings.scrollback_lines, usize::MAX);
        assert_eq!(settings.input.shift_enter, None);
        assert!(settings.copy_on_select);
    }

    #[test]
    fn home_relative_working_directories_expand() {
        let settings = settings("[terminal]\nworking_directory = \"~/projects\"\n");
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME set");
        assert_eq!(settings.working_directory, Some(home.join("projects")));
    }
}

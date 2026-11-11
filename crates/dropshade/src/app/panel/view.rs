//! The settings panel's layout: a section list on the left, the section's settings on the right.

use dropshade_config::{Config, CursorShape, TabBarPosition, TabWidth};
use dropshade_term::colors::to_iced;
use iced::widget::{
    Column, Space, button, column, container, pick_list, row, scrollable, slider, text, text_input,
    toggler,
};
use iced::{Color, Element, Fill, Font, Length, Padding, font};

use super::fields::{self, Field, Slider, Switch};
use super::{BindingKind, Choice, LayoutChoice, Section, SettingsMessage, SettingsPanel};
use crate::app::{App, Host, Message};
use crate::hotkey;
use crate::theme::UiColors;

const MAX_WIDTH: f32 = 720.0;
const MAX_HEIGHT: f32 = 560.0;
const MARGIN: f32 = 24.0;
const SIDEBAR_WIDTH: f32 = 150.0;
pub(super) const LABEL_WIDTH: f32 = 170.0;
const VALUE_WIDTH: f32 = 56.0;
pub(super) const TEXT_SIZE: f32 = 13.0;
const HINT_SIZE: f32 = 11.5;
const BAR_POSITIONS: [&str; 4] = ["top", "bottom", "left", "right"];
const TAB_WIDTHS: [&str; 2] = ["fixed", "fit"];
const CURSOR_SHAPES: [&str; 3] = ["block", "beam", "underline"];

/// Everything a section needs to draw itself.
#[derive(Clone, Copy)]
pub(super) struct Ctx<'a> {
    pub(super) panel: &'a SettingsPanel,
    pub(super) config: &'a Config,
    pub(super) colors: UiColors,
}

pub(super) fn settings(message: SettingsMessage) -> Message {
    Message::Settings(message)
}

impl App {
    pub(in crate::app) fn settings_layer(&self, panel: &SettingsPanel) -> Element<'_, Message> {
        let colors = self.settings.colors;
        let ctx = Ctx {
            panel,
            config: &self.config,
            colors,
        };
        let page = scrollable(
            column![self.warnings(), self.section(ctx)]
                .spacing(14)
                .padding(Padding::new(20.0).right(28.0)),
        )
        .height(Fill);
        let main = column![page, self.footer(panel)].width(Fill);
        let card = container(row![sidebar(panel.section, colors), divider(colors), main])
            .width((self.window_size.width - MARGIN).clamp(0.0, MAX_WIDTH))
            .height((self.window_size.height - MARGIN).clamp(0.0, MAX_HEIGHT))
            .style(move |_| colors.surface_style());
        self.modal(
            container(card)
                .center(Fill)
                .style(move |_| colors.backdrop_style()),
        )
    }

    fn section(&self, ctx: Ctx<'_>) -> Element<'_, Message> {
        let heading = text(ctx.panel.section.title())
            .size(18)
            .font(bold())
            .color(ctx.colors.text);
        let body = match ctx.panel.section {
            Section::Appearance => appearance(ctx),
            Section::Colors => {
                super::colors_page::colors(ctx, self.prefers_light, &self.settings.style)
            }
            Section::Font => font_section(ctx),
            Section::Window => {
                let hotkey = if self.hotkey_settable() {
                    HotkeyField::Shown(self.hotkey.status.as_ref())
                } else {
                    HotkeyField::Hidden
                };
                window(ctx, self.host, hotkey, self.danger())
            }
            Section::Terminal => terminal(ctx),
            Section::Input => input(ctx),
            Section::Session => session(ctx),
            Section::Keybindings => keybindings(ctx),
        };
        column![heading, body].spacing(16).into()
    }

    /// Problems in the current settings that did not stop them from loading, like a missing font.
    fn warnings(&self) -> Element<'_, Message> {
        let color = self.danger();
        Column::with_children(
            self.settings
                .warnings
                .iter()
                .map(|warning| text(warning.as_str()).size(HINT_SIZE).color(color).into()),
        )
        .spacing(4)
        .into()
    }

    fn footer(&self, panel: &SettingsPanel) -> Element<'_, Message> {
        let colors = self.settings.colors;
        let status = match &panel.error {
            Some(error) => text(error.clone()).size(12).color(self.danger()),
            None => text("Changes are saved to config.toml right away.")
                .size(12)
                .color(colors.muted),
        };
        let done = button(text("Done").size(TEXT_SIZE))
            .padding([7, 16])
            .style(move |_, status| colors.accent_button(status))
            .on_press(settings(SettingsMessage::Close));
        container(
            row![container(status).width(Fill), done]
                .spacing(12)
                .align_y(iced::Center),
        )
        .padding([12, 20])
        .into()
    }

    fn danger(&self) -> Color {
        to_iced(self.settings.style.palette.normal[1], 1.0)
    }
}

fn sidebar(current: Section, colors: UiColors) -> Element<'static, Message> {
    let items = Section::ALL.into_iter().map(|section| {
        let active = section == current;
        button(text(section.title()).size(TEXT_SIZE))
            .width(Fill)
            .padding([7, 10])
            .style(move |_, status| colors.nav_button(active, status))
            .on_press(settings(SettingsMessage::Section(section)))
            .into()
    });
    let open = button(text("Open config file").size(12))
        .width(Fill)
        .padding([7, 10])
        .style(move |_, status| colors.ghost_button(colors.muted, status))
        .on_press(settings(SettingsMessage::OpenFile));
    column![
        text("Settings").size(16).font(bold()),
        Space::new().height(8),
        Column::with_children(items).spacing(2),
        Space::new().height(Fill),
        open,
    ]
    .width(SIDEBAR_WIDTH)
    .height(Fill)
    .padding(14)
    .spacing(4)
    .into()
}

fn divider(colors: UiColors) -> Element<'static, Message> {
    container(Space::new())
        .width(1)
        .height(Fill)
        .style(move |_| container::Style::default().background(colors.border))
        .into()
}

fn appearance(ctx: Ctx<'_>) -> Element<'static, Message> {
    let appearance = &ctx.config.appearance;
    let cursor = setting(
        "Cursor",
        row![
            pick_list(
                strings(&CURSOR_SHAPES),
                Some(cursor_name(appearance.cursor_shape).to_owned()),
                |shape| settings(SettingsMessage::Choose(Choice::Cursor, shape)),
            )
            .text_size(TEXT_SIZE),
            toggler(appearance.cursor_blink)
                .label("Blink")
                .text_size(TEXT_SIZE)
                .on_toggle(|on| settings(SettingsMessage::Toggle(Switch::CursorBlink, on))),
        ]
        .spacing(12)
        .align_y(iced::Center),
    );
    column![
        hint("Themes and colors are on the Colors page.", ctx.colors),
        slider_row(ctx, Slider::Opacity),
        slider_row(ctx, Slider::Padding),
        choice(
            "Tab bar",
            strings(&BAR_POSITIONS),
            bar_name(appearance.tab_bar),
            Choice::TabBar
        ),
        hinted(
            choice(
                "Tab width",
                strings(&TAB_WIDTHS),
                tab_width_name(appearance.tab_width),
                Choice::TabWidth
            ),
            "\"fit\" sizes top and bottom tabs to their names.",
            ctx.colors,
        ),
        cursor,
        switch(ctx, "Scrollbar", Switch::Scrollbar),
    ]
    .spacing(12)
    .into()
}

fn font_section(ctx: Ctx<'_>) -> Element<'static, Message> {
    column![
        hinted(
            text_row(ctx, "Family", Field::FontFamily, "monospace"),
            "An installed font family, or \"monospace\" for the system default.",
            ctx.colors,
        ),
        slider_row(ctx, Slider::FontSize),
        slider_row(ctx, Slider::LineHeight),
    ]
    .spacing(12)
    .into()
}

/// On layer-shell desktops, whether the hotkey can be set, with its binding once known.
enum HotkeyField<'a> {
    Hidden,
    Shown(Option<&'a (hotkey::Binder, hotkey::Status)>),
}

fn window(
    ctx: Ctx<'_>,
    host: Host,
    hotkey: HotkeyField<'_>,
    danger: Color,
) -> Element<'static, Message> {
    let field = || {
        hinted(
            text_row(ctx, "Hotkey", Field::Hotkey, "F12"),
            "GNOME accelerator syntax, such as F12 or <Super>grave.",
            ctx.colors,
        )
    };
    let applies = "Changes apply the next time the terminal drops down.";
    let (note, hotkey): (Element<'static, Message>, Element<'static, Message>) =
        match (host, hotkey) {
            (Host::Shell, _) => (Space::new().into(), field()),
            (Host::Layer, HotkeyField::Shown(bound)) => {
                let status = bound.map_or_else(
                    || Space::new().into(),
                    |(binder, status)| binding_status(*binder, status, ctx.colors, danger),
                );
                (
                    hint(applies, ctx.colors),
                    column![
                        field(),
                        row![Space::new().width(LABEL_WIDTH + 10.0), status]
                    ]
                    .spacing(4)
                    .into(),
                )
            }
            (Host::Layer, HotkeyField::Hidden) => (
                hint(
                    "Bind \"dropshade toggle\" to a key in your desktop's keyboard settings. \
                     Changes apply the next time the terminal drops down.",
                    ctx.colors,
                ),
                Space::new().into(),
            ),
            (Host::Window, _) => (
                hint(
                    "These apply when the GNOME Shell extension shows Dropshade.",
                    ctx.colors,
                ),
                field(),
            ),
        };
    column![
        note,
        hotkey,
        super::screen_list::screen_list(ctx),
        slider_row(ctx, Slider::Width),
        slider_row(ctx, Slider::Height),
        slider_row(ctx, Slider::Animation),
        switch(ctx, "Hide when unfocused", Switch::HideOnUnfocus),
    ]
    .spacing(12)
    .into()
}

/// Which keys the desktop runs Dropshade on, and why not if they could not be set.
fn binding_status(
    binder: hotkey::Binder,
    status: &hotkey::Status,
    colors: UiColors,
    danger: Color,
) -> Element<'static, Message> {
    use hotkey::{Binder, Status};
    let (message, color) = match (binder, status) {
        (Binder::Cosmic, Status::Bound(keys)) => (
            format!("COSMIC runs Dropshade on {keys}. Press Enter in the field to change it."),
            colors.muted,
        ),
        (Binder::Cosmic, Status::Unbound) => (
            "Not in COSMIC's shortcuts yet: press Enter in the field to add it.".to_owned(),
            colors.accent,
        ),
        (Binder::Cosmic, Status::Failed(error)) => (
            format!("COSMIC's shortcuts were left alone: {error}"),
            danger,
        ),
        (Binder::Portal, Status::Bound(keys)) if keys.is_empty() => (
            "Registered with the desktop as \"Show or hide Dropshade\"; assign it a key in the \
             desktop's shortcut settings."
                .to_owned(),
            colors.muted,
        ),
        (Binder::Portal, Status::Bound(keys)) => (
            format!(
                "The desktop toggles Dropshade on {keys} while it runs. The desktop has the final \
                 say: if it keeps another key, change it in its shortcut settings."
            ),
            colors.muted,
        ),
        (Binder::Portal, Status::Unbound) => (
            "The desktop did not add the shortcut. Press Enter in the field to ask again."
                .to_owned(),
            colors.accent,
        ),
        (Binder::Portal, Status::Failed(error)) => (
            format!("The desktop's shortcut portal failed: {error}"),
            danger,
        ),
    };
    text(message).size(HINT_SIZE).color(color).into()
}

fn terminal(ctx: Ctx<'_>) -> Element<'static, Message> {
    let unlimited = Switch::UnlimitedScrollback.current(ctx.config);
    let lines = text_input(
        if unlimited { "unlimited" } else { "10000" },
        &if unlimited {
            String::new()
        } else {
            ctx.panel.text(Field::Scrollback, ctx.config)
        },
    )
    .size(TEXT_SIZE)
    .padding([6, 8])
    .width(110)
    .on_input_maybe((!unlimited).then_some(|text| edit(Field::Scrollback, text)))
    .on_submit(settings(SettingsMessage::Submit(Field::Scrollback)));
    let scrollback = setting(
        "Scrollback lines",
        row![
            lines,
            toggler(unlimited)
                .label("Unlimited")
                .text_size(TEXT_SIZE)
                .on_toggle(|on| settings(SettingsMessage::Toggle(Switch::UnlimitedScrollback, on))),
            unsaved_mark(ctx, Field::Scrollback),
        ]
        .spacing(12)
        .align_y(iced::Center),
    );
    column![
        hint("Shell changes apply to new tabs.", ctx.colors),
        text_row(ctx, "Shell", Field::Shell, "Login shell"),
        hinted(
            text_row(ctx, "Arguments", Field::ShellArgs, "None"),
            "Quoted as in a shell, e.g. -l -c \"tmux new\".",
            ctx.colors,
        ),
        hinted(
            text_row(
                ctx,
                "Working directory",
                Field::WorkingDirectory,
                "Home directory"
            ),
            "Where the first tab starts; ~ is your home directory.",
            ctx.colors,
        ),
        scrollback,
        subheading("Environment", ctx.colors),
        environment(ctx),
    ]
    .spacing(12)
    .into()
}

fn environment(ctx: Ctx<'_>) -> Element<'static, Message> {
    let rows = ctx.config.terminal.env.iter().map(|(name, value)| {
        entry_row(
            name,
            value.clone(),
            SettingsMessage::RemoveEnv(name.clone()),
            ctx.colors,
        )
    });
    let add = row![
        draft_input(ctx, Field::EnvName, "NAME").width(LABEL_WIDTH),
        draft_input(ctx, Field::EnvValue, "value").width(Fill),
        add_button(SettingsMessage::AddEnv, ctx.colors),
    ]
    .spacing(8)
    .align_y(iced::Center);
    Column::with_children(rows).push(add).spacing(6).into()
}

fn input(ctx: Ctx<'_>) -> Element<'static, Message> {
    column![
        hinted(
            text_row(ctx, "Shift+Enter sends", Field::ShiftEnter, "A plain Enter"),
            "Outside the kitty keyboard protocol. Escapes: \\e Esc, \\r Enter, \\n, \\t, \\x07.",
            ctx.colors,
        ),
        switch(ctx, "Input method (IME)", Switch::Ime),
        hinted(
            switch(ctx, "Copy on select", Switch::CopyOnSelect),
            "Selections always go to the primary selection; this also copies them to the clipboard.",
            ctx.colors,
        ),
    ]
    .spacing(12)
    .into()
}

fn session(ctx: Ctx<'_>) -> Element<'static, Message> {
    let current = &ctx.config.session.layout;
    let mut choices = vec![LayoutChoice::None];
    choices.extend(ctx.panel.layouts.iter().cloned().map(LayoutChoice::Named));
    if !current.is_empty() && !ctx.panel.layouts.contains(current) {
        choices.push(LayoutChoice::Named(current.clone()));
    }
    let selected = if current.is_empty() {
        LayoutChoice::None
    } else {
        LayoutChoice::Named(current.clone())
    };
    column![
        switch(ctx, "Reopen tabs on start", Switch::Restore),
        hinted(
            setting(
                "Start with layout",
                pick_list(choices, Some(selected), |choice| settings(
                    SettingsMessage::Layout(choice)
                ))
                .text_size(TEXT_SIZE),
            ),
            "A layout replaces the reopened tabs. Save one with: dropshade layout save NAME",
            ctx.colors,
        ),
        hinted(
            switch(ctx, "Check for updates", Switch::CheckUpdates),
            "Asks GitHub for the latest release on start and notifies you once per new version.",
            ctx.colors,
        ),
    ]
    .spacing(12)
    .into()
}

fn keybindings(ctx: Ctx<'_>) -> Element<'static, Message> {
    let colors = ctx.colors;
    let rows = ctx.config.keybindings.iter().map(|(chord, binding)| {
        entry_row(
            chord,
            fields::describe_binding(binding),
            SettingsMessage::RemoveBinding(chord.clone()),
            colors,
        )
    });
    let target: Element<'static, Message> = match ctx.panel.binding_kind {
        BindingKind::Action => pick_list(
            fields::action_names(),
            Some(ctx.panel.binding_action.clone()),
            |action| settings(SettingsMessage::BindingAction(action)),
        )
        .text_size(TEXT_SIZE)
        .width(Fill)
        .into(),
        BindingKind::Send => draft_input(ctx, Field::SendText, "clear\\r")
            .width(Fill)
            .into(),
    };
    let add = row![
        draft_input(ctx, Field::Chord, "ctrl+shift+k").width(LABEL_WIDTH),
        pick_list(
            [BindingKind::Action, BindingKind::Send],
            Some(ctx.panel.binding_kind),
            |kind| settings(SettingsMessage::BindingKind(kind)),
        )
        .text_size(TEXT_SIZE),
        target,
        add_button(SettingsMessage::AddBinding, colors),
    ]
    .spacing(8)
    .align_y(iced::Center);
    let list: Element<'static, Message> = if ctx.config.keybindings.is_empty() {
        hint("No custom keybindings yet.", colors)
    } else {
        Column::with_children(rows).spacing(6).into()
    };
    column![
        hint(
            "Custom chords are checked before the built-in shortcuts. Bind a built-in chord \
             to \"none\" to pass it to the terminal instead.",
            colors,
        ),
        list,
        subheading("Add a keybinding", colors),
        add,
    ]
    .spacing(12)
    .into()
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

fn hinted(
    control: Element<'static, Message>,
    note: &'static str,
    colors: UiColors,
) -> Element<'static, Message> {
    column![
        control,
        row![Space::new().width(LABEL_WIDTH + 10.0), hint(note, colors)]
    ]
    .spacing(4)
    .into()
}

pub(super) fn hint(note: &'static str, colors: UiColors) -> Element<'static, Message> {
    text(note).size(HINT_SIZE).color(colors.muted).into()
}

pub(super) fn subheading(title: &'static str, colors: UiColors) -> Element<'static, Message> {
    container(text(title).size(TEXT_SIZE).font(bold()).color(colors.text))
        .padding(Padding::ZERO.top(6.0))
        .into()
}

fn slider_row(ctx: Ctx<'_>, which: Slider) -> Element<'static, Message> {
    let spec = which.spec();
    let value = ctx.panel.slider(which, ctx.config);
    setting(
        spec.label,
        row![
            slider(spec.min..=spec.max, value, move |v| settings(
                SettingsMessage::Slide(which, v)
            ))
            .step(spec.step)
            .on_release(settings(SettingsMessage::Release(which))),
            text(which.shown(value))
                .size(TEXT_SIZE)
                .color(ctx.colors.muted)
                .width(Length::Fixed(VALUE_WIDTH)),
        ]
        .spacing(8)
        .align_y(iced::Center),
    )
}

pub(super) fn switch(ctx: Ctx<'_>, name: &'static str, which: Switch) -> Element<'static, Message> {
    setting(
        name,
        toggler(which.current(ctx.config))
            .on_toggle(move |on| settings(SettingsMessage::Toggle(which, on))),
    )
}

pub(super) fn choice(
    name: &'static str,
    options: Vec<String>,
    selected: &str,
    which: Choice,
) -> Element<'static, Message> {
    setting(
        name,
        pick_list(options, Some(selected.to_owned()), move |value| {
            settings(SettingsMessage::Choose(which, value))
        })
        .text_size(TEXT_SIZE),
    )
}

fn edit(field: Field, text: String) -> Message {
    settings(SettingsMessage::Edit(field, text))
}

/// A text field for one setting, saved on Enter (or when the panel closes).
fn text_row(
    ctx: Ctx<'_>,
    name: &'static str,
    field: Field,
    placeholder: &'static str,
) -> Element<'static, Message> {
    setting(
        name,
        row![
            draft_input(ctx, field, placeholder).width(Fill),
            unsaved_mark(ctx, field),
        ]
        .spacing(8)
        .align_y(iced::Center),
    )
}

fn draft_input<'a>(
    ctx: Ctx<'_>,
    field: Field,
    placeholder: &str,
) -> iced::widget::TextInput<'a, Message> {
    text_input(placeholder, &ctx.panel.text(field, ctx.config))
        .size(TEXT_SIZE)
        .padding([6, 8])
        .on_input(move |text| edit(field, text))
        .on_submit(settings(SettingsMessage::Submit(field)))
}

fn unsaved_mark(ctx: Ctx<'_>, field: Field) -> Element<'static, Message> {
    if ctx.panel.unsaved.contains(&field) {
        text("↵ to save")
            .size(HINT_SIZE)
            .color(ctx.colors.accent)
            .into()
    } else {
        Space::new().into()
    }
}

fn entry_row(
    name: &str,
    value: String,
    on_remove: SettingsMessage,
    colors: UiColors,
) -> Element<'static, Message> {
    row![
        text(name.to_owned())
            .size(TEXT_SIZE)
            .font(Font::MONOSPACE)
            .width(Length::Fixed(LABEL_WIDTH)),
        text(value).size(TEXT_SIZE).color(colors.muted).width(Fill),
        button(text("Remove").size(12))
            .padding([4, 10])
            .style(move |_, status| colors.ghost_button(colors.muted, status))
            .on_press(settings(on_remove)),
    ]
    .spacing(10)
    .align_y(iced::Center)
    .into()
}

fn add_button(message: SettingsMessage, colors: UiColors) -> Element<'static, Message> {
    button(text("Add").size(TEXT_SIZE))
        .padding([6, 14])
        .style(move |_, status| colors.ghost_button(colors.text, status))
        .on_press(settings(message))
        .into()
}

pub(super) fn bold() -> Font {
    Font {
        weight: font::Weight::Bold,
        ..Font::DEFAULT
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn bar_name(position: TabBarPosition) -> &'static str {
    match position {
        TabBarPosition::Top => "top",
        TabBarPosition::Bottom => "bottom",
        TabBarPosition::Left => "left",
        TabBarPosition::Right => "right",
    }
}

fn tab_width_name(width: TabWidth) -> &'static str {
    match width {
        TabWidth::Fixed => "fixed",
        TabWidth::Fit => "fit",
    }
}

fn cursor_name(shape: CursorShape) -> &'static str {
    match shape {
        CursorShape::Block => "block",
        CursorShape::Beam => "beam",
        CursorShape::Underline => "underline",
    }
}

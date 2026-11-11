//! The Colors page: the base theme, a preview, and every terminal color with its picker.

use dropshade_config::{ColorOverrides, ThemeChoice, theme};
use dropshade_term::ViewStyle;
use dropshade_term::alacritty_terminal::vte::ansi::Rgb;
use dropshade_term::colors::{CellColors, to_iced};
use iced::widget::text::Span;
use iced::widget::{Column, Space, button, column, container, rich_text, row, span, text, tooltip};
use iced::{Background, Border, Element, Fill, Font, Length, font};

use super::color_picker::{open_in, picker, swatch};
use super::color_slots::ColorSlot;
use super::fields::Switch;
use super::hsv::Hsv;
use super::view::{Ctx, LABEL_WIDTH, TEXT_SIZE, choice, hint, settings, subheading, switch};
use super::{Choice, SettingsMessage};
use crate::app::Message;
use crate::theme::RADIUS;

const PRIMARY_SWATCH: f32 = 22.0;
const PALETTE_SWATCH: f32 = 26.0;

pub(super) fn colors(
    ctx: Ctx<'_>,
    prefers_light: bool,
    style: &ViewStyle,
) -> Element<'static, Message> {
    let palette = &style.palette;
    let primary = ColorSlot::PRIMARY.map(|slot| primary_row(ctx, style, slot));
    let mut page = column![
        base_theme(ctx, prefers_light),
        preview(style),
        subheading("Terminal colors", ctx.colors),
        hint(
            "Click a color to change it. Changed colors stay when you switch themes.",
            ctx.colors,
        ),
        Column::with_children(primary).spacing(8),
        subheading("Palette", ctx.colors),
        palette_rows(ctx, style),
    ]
    .spacing(12);
    if let Some(state) = open_in(ctx.panel.picker.as_ref(), palette_slots()) {
        page = page.push(picker(ctx, state, palette));
    }
    if ctx.config.colors != ColorOverrides::default() {
        page = page.push(
            button(
                text(if ctx.panel.confirm_reset_colors {
                    "Click again to reset all colors"
                } else {
                    "Reset all colors to the theme"
                })
                .size(12),
            )
            .padding([6, 12])
            .style(move |_, status| ctx.colors.ghost_button(ctx.colors.muted, status))
            .on_press(settings(SettingsMessage::ResetAllColors)),
        );
    }
    page.into()
}

fn base_theme(ctx: Ctx<'_>, prefers_light: bool) -> Element<'static, Message> {
    let theme = &ctx.config.appearance.theme;
    let pickers: Element<'static, Message> = match theme {
        ThemeChoice::Single(_) => choice(
            "Theme",
            theme_names(),
            theme.name(prefers_light),
            Choice::Theme,
        ),
        ThemeChoice::FollowSystem { dark, light } => column![
            choice("Dark theme", theme_names(), dark, Choice::DarkTheme),
            choice("Light theme", theme_names(), light, Choice::LightTheme),
        ]
        .spacing(12)
        .into(),
    };
    column![
        switch(ctx, "Follow system light/dark", Switch::FollowSystem),
        pickers
    ]
    .spacing(12)
    .into()
}

/// A few lines of shell output in the current colors.
fn preview(style: &ViewStyle) -> Element<'static, Message> {
    let palette = &style.palette;
    let font = style.font;
    let bold = Font {
        weight: font::Weight::Bold,
        ..font
    };
    let fg = palette.foreground;
    let colored = |content: &'static str, color: Rgb| -> Span<'static, ()> {
        span(content).color(to_iced(color, 1.0)).font(font)
    };
    let selection = palette.selected(CellColors {
        fg,
        bg: palette.background,
    });
    let lines: [Vec<Span<'static, ()>>; 4] = [
        vec![
            colored("user@host", palette.normal[2]).font(bold),
            colored(":", fg),
            colored("~/projects", palette.normal[4]).font(bold),
            colored("$ ls", fg),
        ],
        vec![
            colored("Cargo.toml  ", fg),
            colored("crates/  ", palette.bright[4]).font(bold),
            colored("build.sh  ", palette.normal[2]),
            colored("notes.md", palette.bright[0]),
        ],
        vec![
            colored("error", palette.normal[1]).font(bold),
            colored(": ", fg),
            colored("warning", palette.normal[3]),
            colored(": ", fg),
            colored("note", palette.normal[6]),
            colored(" v0.3.0 ", palette.normal[5]),
            colored("~ ", palette.bright[1]),
            colored("ok", palette.bright[2]),
        ],
        vec![
            colored("$ echo ", fg),
            colored("selected", selection.fg).background(to_iced(selection.bg, 1.0)),
            colored(" text", fg),
            colored("_", palette.cursor_text()).background(to_iced(palette.cursor, 1.0)),
        ],
    ];
    let background = to_iced(palette.background, 1.0);
    container(
        Column::with_children(lines.map(|line| rich_text(line).size(TEXT_SIZE).into())).spacing(2),
    )
    .width(Fill)
    .padding([10, 12])
    .style(move |_| container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            radius: RADIUS.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn primary_row(ctx: Ctx<'_>, style: &ViewStyle, slot: ColorSlot) -> Element<'static, Message> {
    let palette = &style.palette;
    let open = open_in(ctx.panel.picker.as_ref(), [slot]);
    let color = shown(ctx, style, slot);
    let configured = slot.configured(&ctx.config.colors).is_some();
    let note: Element<'static, Message> = if configured {
        button(text("Reset").size(12))
            .padding([3, 8])
            .style(move |_, status| ctx.colors.ghost_button(ctx.colors.muted, status))
            .on_press(settings(SettingsMessage::ResetColor(slot)))
            .into()
    } else if slot.automatic(palette) {
        hint("automatic", ctx.colors)
    } else {
        Space::new().into()
    };
    let line = row![
        text(slot.label())
            .size(TEXT_SIZE)
            .width(Length::Fixed(LABEL_WIDTH)),
        swatch(
            color,
            PRIMARY_SWATCH,
            open.is_some(),
            ctx.colors,
            SettingsMessage::OpenPicker(slot)
        ),
        text(color.to_hex())
            .size(TEXT_SIZE)
            .font(Font::MONOSPACE)
            .color(ctx.colors.muted),
        note,
    ]
    .spacing(10)
    .align_y(iced::Center);
    match open {
        Some(state) => column![line, picker(ctx, state, palette)].spacing(8).into(),
        None => line.into(),
    }
}

fn palette_rows(ctx: Ctx<'_>, style: &ViewStyle) -> Element<'static, Message> {
    let open = ctx.panel.picker.as_ref().map(|state| state.slot);
    let line = |name: &'static str, slot: fn(usize) -> ColorSlot| {
        let swatches = (0..8).map(|index| {
            let slot = slot(index);
            let marked = slot.configured(&ctx.config.colors).is_some();
            let label = if marked {
                format!("{} (changed)", slot.label())
            } else {
                slot.label()
            };
            let sample = swatch(
                shown(ctx, style, slot),
                PALETTE_SWATCH,
                open == Some(slot),
                ctx.colors,
                SettingsMessage::OpenPicker(slot),
            );
            tooltip(
                sample,
                container(text(label).size(12))
                    .padding([4, 8])
                    .style(move |_| ctx.colors.surface_style()),
                tooltip::Position::Top,
            )
            .into()
        });
        row![
            text(name).size(TEXT_SIZE).width(Length::Fixed(LABEL_WIDTH)),
            iced::widget::Row::with_children(swatches).spacing(6),
        ]
        .spacing(10)
        .align_y(iced::Center)
    };
    column![
        line("Normal", ColorSlot::Normal),
        line("Bright", ColorSlot::Bright)
    ]
    .spacing(8)
    .into()
}

/// The color a slot shows: the picker's draft while it is being edited, else the one in use.
fn shown(ctx: Ctx<'_>, style: &ViewStyle, slot: ColorSlot) -> theme::Rgb8 {
    open_in(ctx.panel.picker.as_ref(), [slot])
        .and_then(|state| state.draft)
        .map_or_else(|| slot.effective(&style.palette), Hsv::to_rgb)
}

fn palette_slots() -> impl Iterator<Item = ColorSlot> {
    ColorSlot::all().filter(|slot| matches!(slot, ColorSlot::Normal(_) | ColorSlot::Bright(_)))
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

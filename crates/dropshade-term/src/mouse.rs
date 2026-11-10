//! Encodes mouse events for applications that enable mouse reporting (xterm modes 1000–1006).

use alacritty_terminal::term::TermMode;
use iced::keyboard::Modifiers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Press(Button),
    Release(Button),
    /// Pointer moved to another cell, possibly while a button is held.
    Motion(Option<Button>),
    WheelUp,
    WheelDown,
}

/// A cell in the visible viewport, zero-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellPosition {
    pub column: usize,
    pub line: usize,
}

/// Whether mouse events go to the application. Holding Shift keeps them for local selection.
pub fn reporting_active(mode: TermMode, modifiers: Modifiers) -> bool {
    mode.intersects(TermMode::MOUSE_MODE) && !modifiers.shift()
}

const MOTION_FLAG: u32 = 32;
const RELEASE_CODE: u32 = 3;
const NORMAL_OFFSET: u32 = 32;
const NORMAL_MAX: u32 = 255;
const UTF8_MAX: u32 = 2047;

/// The bytes reporting `action` at `position`, or `None` when the current mode does not report it.
pub fn encode_report(
    action: MouseAction,
    position: CellPosition,
    modifiers: Modifiers,
    mode: TermMode,
) -> Option<Vec<u8>> {
    if !is_reported(action, mode) {
        return None;
    }
    let sgr = mode.contains(TermMode::SGR_MOUSE);
    let code = button_code(action, sgr) + modifier_bits(modifiers);
    let column = u32::try_from(position.column).ok()?.checked_add(1)?;
    let line = u32::try_from(position.line).ok()?.checked_add(1)?;

    if sgr {
        let suffix = if matches!(action, MouseAction::Release(_)) {
            'm'
        } else {
            'M'
        };
        return Some(format!("\x1b[<{code};{column};{line}{suffix}").into_bytes());
    }
    let utf8 = mode.contains(TermMode::UTF8_MOUSE);
    let mut bytes = b"\x1b[M".to_vec();
    for value in [code, column, line] {
        push_normal(&mut bytes, value + NORMAL_OFFSET, utf8)?;
    }
    Some(bytes)
}

fn is_reported(action: MouseAction, mode: TermMode) -> bool {
    match action {
        MouseAction::Motion(None) => mode.contains(TermMode::MOUSE_MOTION),
        MouseAction::Motion(Some(_)) => {
            mode.intersects(TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG)
        }
        _ => mode.intersects(TermMode::MOUSE_MODE),
    }
}

fn button_code(action: MouseAction, sgr: bool) -> u32 {
    let button = |button: Button| match button {
        Button::Left => 0,
        Button::Middle => 1,
        Button::Right => 2,
    };
    match action {
        MouseAction::Press(pressed) => button(pressed),
        MouseAction::Release(released) if sgr => button(released),
        MouseAction::Release(_) => RELEASE_CODE,
        MouseAction::Motion(held) => MOTION_FLAG + held.map_or(RELEASE_CODE, button),
        MouseAction::WheelUp => 64,
        MouseAction::WheelDown => 65,
    }
}

fn modifier_bits(modifiers: Modifiers) -> u32 {
    4 * u32::from(modifiers.shift())
        + 8 * u32::from(modifiers.alt())
        + 16 * u32::from(modifiers.control())
}

/// Appends one value of the legacy encoding, which is a single byte (or a UTF-8 char in 1005 mode).
fn push_normal(bytes: &mut Vec<u8>, value: u32, utf8: bool) -> Option<()> {
    if utf8 && value <= UTF8_MAX {
        let mut buffer = [0; 4];
        bytes.extend(char::from_u32(value)?.encode_utf8(&mut buffer).as_bytes());
        return Some(());
    }
    bytes.push(u8::try_from(value).ok().filter(|_| value <= NORMAL_MAX)?);
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: CellPosition = CellPosition { column: 0, line: 0 };
    const CLICK_SGR: TermMode = TermMode::MOUSE_REPORT_CLICK.union(TermMode::SGR_MOUSE);

    fn report(
        action: MouseAction,
        position: CellPosition,
        mods: Modifiers,
        mode: TermMode,
    ) -> Option<Vec<u8>> {
        encode_report(action, position, mods, mode)
    }

    #[test]
    fn nothing_is_reported_without_a_mouse_mode() {
        let press = MouseAction::Press(Button::Left);
        assert_eq!(
            report(press, ORIGIN, Modifiers::empty(), TermMode::SGR_MOUSE),
            None
        );
    }

    #[test]
    fn sgr_reports_press_and_release_with_one_based_coordinates() {
        let at = CellPosition { column: 4, line: 2 };
        let press = report(
            MouseAction::Press(Button::Left),
            at,
            Modifiers::empty(),
            CLICK_SGR,
        );
        assert_eq!(press, Some(b"\x1b[<0;5;3M".to_vec()));
        let release = report(
            MouseAction::Release(Button::Left),
            at,
            Modifiers::empty(),
            CLICK_SGR,
        );
        assert_eq!(release, Some(b"\x1b[<0;5;3m".to_vec()));
    }

    #[test]
    fn normal_encoding_offsets_by_32_and_releases_as_button_3() {
        let at = CellPosition { column: 9, line: 4 };
        let mode = TermMode::MOUSE_REPORT_CLICK;
        let press = report(
            MouseAction::Press(Button::Right),
            at,
            Modifiers::empty(),
            mode,
        );
        assert_eq!(press, Some(vec![0x1b, b'[', b'M', 32 + 2, 32 + 10, 32 + 5]));
        let release = report(
            MouseAction::Release(Button::Right),
            at,
            Modifiers::empty(),
            mode,
        );
        assert_eq!(
            release,
            Some(vec![0x1b, b'[', b'M', 32 + 3, 32 + 10, 32 + 5])
        );
    }

    #[test]
    fn modifiers_are_added_to_the_button_code() {
        let ctrl = report(
            MouseAction::Press(Button::Left),
            ORIGIN,
            Modifiers::CTRL,
            CLICK_SGR,
        );
        assert_eq!(ctrl, Some(b"\x1b[<16;1;1M".to_vec()));
        let alt = report(
            MouseAction::Press(Button::Middle),
            ORIGIN,
            Modifiers::ALT,
            CLICK_SGR,
        );
        assert_eq!(alt, Some(b"\x1b[<9;1;1M".to_vec()));
    }

    #[test]
    fn wheel_is_reported_as_buttons_64_and_65() {
        let up = report(MouseAction::WheelUp, ORIGIN, Modifiers::empty(), CLICK_SGR);
        assert_eq!(up, Some(b"\x1b[<64;1;1M".to_vec()));
        let down = report(
            MouseAction::WheelDown,
            ORIGIN,
            Modifiers::empty(),
            TermMode::MOUSE_REPORT_CLICK,
        );
        assert_eq!(down, Some(vec![0x1b, b'[', b'M', 32 + 65, 33, 33]));
    }

    #[test]
    fn drag_mode_reports_motion_only_while_a_button_is_held() {
        let mode = TermMode::MOUSE_DRAG | TermMode::SGR_MOUSE;
        assert_eq!(
            report(MouseAction::Motion(None), ORIGIN, Modifiers::empty(), mode),
            None
        );
        let drag = report(
            MouseAction::Motion(Some(Button::Left)),
            ORIGIN,
            Modifiers::empty(),
            mode,
        );
        assert_eq!(drag, Some(b"\x1b[<32;1;1M".to_vec()));
    }

    #[test]
    fn motion_mode_reports_hover_as_button_3() {
        let mode = TermMode::MOUSE_MOTION | TermMode::SGR_MOUSE;
        let hover = report(MouseAction::Motion(None), ORIGIN, Modifiers::empty(), mode);
        assert_eq!(hover, Some(b"\x1b[<35;1;1M".to_vec()));
    }

    #[test]
    fn click_mode_does_not_report_motion() {
        let drag = MouseAction::Motion(Some(Button::Left));
        assert_eq!(report(drag, ORIGIN, Modifiers::empty(), CLICK_SGR), None);
    }

    #[test]
    fn normal_encoding_cannot_express_far_columns_but_utf8_can() {
        let far = CellPosition {
            column: 299,
            line: 0,
        };
        let press = MouseAction::Press(Button::Left);
        assert_eq!(
            report(press, far, Modifiers::empty(), TermMode::MOUSE_REPORT_CLICK),
            None
        );

        let utf8 = TermMode::MOUSE_REPORT_CLICK | TermMode::UTF8_MOUSE;
        let mut expected = vec![0x1b, b'[', b'M', 32];
        expected.extend("\u{14c}".as_bytes());
        expected.push(33);
        assert_eq!(report(press, far, Modifiers::empty(), utf8), Some(expected));
    }

    #[test]
    fn shift_keeps_the_mouse_for_selection() {
        assert!(reporting_active(
            TermMode::MOUSE_REPORT_CLICK,
            Modifiers::empty()
        ));
        assert!(!reporting_active(
            TermMode::MOUSE_REPORT_CLICK,
            Modifiers::SHIFT
        ));
        assert!(!reporting_active(TermMode::empty(), Modifiers::empty()));
    }
}

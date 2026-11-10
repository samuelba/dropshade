//! The drop-down as a layer-shell surface, for compositors that offer `zwlr_layer_shell_v1`
//! (COSMIC, KDE Plasma, Sway, Hyprland, niri, …). Dropshade places and slides it itself.
//!
//! A new surface first covers the whole work area to learn its size, drawing nothing, then
//! shrinks to the configured fraction and slides in by animating its top margin.

use std::time::{Duration, Instant};

use dropshade_config::Monitor;
use iced::{Size, Task, window};
use iced_layershell::actions::{LayerShellCustomAction, LayerShellCustomActionWithId};
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};

use super::Message;

pub const NAMESPACE: &str = "dropshade";

#[derive(Debug)]
pub struct Dropdown {
    pub id: window::Id,
    /// The surface size, once the work area is known.
    size: Option<(u32, u32)>,
    slide: Slide,
    /// The last tick put the surface where the slide ends.
    settled: bool,
    width: f64,
    height: f64,
}

impl Dropdown {
    /// Creates the surface on the configured screen; it slides in once it knows its size.
    pub fn open(window: &dropshade_config::Window) -> (Self, Task<Message>) {
        let id = window::Id::unique();
        let duration = Duration::from_millis(window.animation_duration.into());
        let dropdown = Self {
            id,
            size: None,
            slide: Slide::new(0.0, 0.0, Instant::now(), duration),
            settled: true,
            width: window.width,
            height: window.height,
        };
        let settings = NewLayerShellSettings {
            size: Some((0, 0)),
            layer: Layer::Top,
            anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
            exclusive_zone: Some(0),
            margin: Some((0, 0, 0, 0)),
            // Exclusive makes the compositor focus the new surface; it relaxes to on-demand
            // once placed so that clicking other windows still works.
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            output_option: output_option(&window.monitor),
            events_transparent: false,
            namespace: Some(NAMESPACE.to_owned()),
        };
        let open = layer(None, LayerShellCustomAction::NewLayerShell { settings, id });
        (dropdown, open)
    }

    /// Still learning the work area: the view draws nothing meanwhile.
    pub fn measuring(&self) -> bool {
        self.size.is_none()
    }

    /// Shown or on its way in.
    pub fn showing(&self) -> bool {
        self.slide.to > 0.0
    }

    /// Still has to move, including the final step to where the slide ends.
    pub fn animating(&self) -> bool {
        self.size.is_some() && !self.settled
    }

    /// The surface reported the work area: shrink to the configured size and start sliding in.
    pub fn placed(&mut self, work_area: Size, now: Instant) -> Task<Message> {
        if self.size.is_some() {
            return Task::none();
        }
        let size = surface_size(work_area, self.width, self.height);
        self.size = Some(size);
        self.slide = Slide::new(0.0, 1.0, now, self.slide.duration);
        self.settled = false;
        Task::batch([
            layer(
                Some(self.id),
                LayerShellCustomAction::AnchorSizeChange(Anchor::Top, size),
            ),
            self.margin(now),
            layer(
                Some(self.id),
                LayerShellCustomAction::KeyboardInteractivityChange(
                    KeyboardInteractivity::OnDemand,
                ),
            ),
        ])
    }

    pub fn show(&mut self, now: Instant) {
        self.slide = self.slide.retarget(1.0, now);
        self.settled = false;
    }

    /// Starts sliding out; [`Self::tick`] reports when it is gone.
    pub fn hide(&mut self, now: Instant) {
        self.slide = self.slide.retarget(0.0, now);
        self.settled = false;
    }

    /// Moves the surface along its slide. Returns whether it finished sliding out.
    pub fn tick(&mut self, now: Instant) -> (Task<Message>, bool) {
        if self.size.is_none() {
            return (Task::none(), !self.showing());
        }
        self.settled = self.slide.done(now);
        let hidden = !self.showing() && self.settled;
        (self.margin(now), hidden)
    }

    fn margin(&self, now: Instant) -> Task<Message> {
        let height = self.size.map_or(0, |(_, height)| height);
        let top = top_margin(height, self.slide.position(now));
        layer(
            Some(self.id),
            LayerShellCustomAction::MarginChange((top, 0, 0, 0)),
        )
    }
}

fn layer(id: Option<window::Id>, action: LayerShellCustomAction) -> Task<Message> {
    Task::done(Message::Layer(LayerShellCustomActionWithId::new(
        id, action,
    )))
}

/// Wayland has no primary screen, so "primary" falls back to the compositor's choice.
fn output_option(monitor: &Monitor) -> OutputOption {
    match monitor {
        Monitor::Pointer | Monitor::Primary => OutputOption::Active,
        Monitor::Connector(name) => OutputOption::OutputName(name.clone()),
    }
}

/// The configured fractions of the work area, in whole logical pixels and at least one.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "fractions of at most 1 of a screen size fit in u32"
)]
fn surface_size(work_area: Size, width: f64, height: f64) -> (u32, u32) {
    let scale = |length: f32, fraction: f64| (f64::from(length) * fraction).round().max(1.0) as u32;
    (
        scale(work_area.width, width),
        scale(work_area.height, height),
    )
}

/// How far above the work area the surface sits when `shown` (0 to 1) of it is visible.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a surface height times a fraction of at most 1 fits in i32"
)]
fn top_margin(height: u32, shown: f32) -> i32 {
    -((f64::from(height) * f64::from(1.0 - shown.clamp(0.0, 1.0))).round() as i32)
}

/// An eased move of the visible fraction from `from` to `to`.
#[derive(Debug, Clone, Copy)]
struct Slide {
    from: f32,
    to: f32,
    since: Instant,
    /// The time a full slide takes; shorter moves take proportionally less.
    duration: Duration,
}

impl Slide {
    fn new(from: f32, to: f32, since: Instant, duration: Duration) -> Self {
        Self {
            from,
            to,
            since,
            duration,
        }
    }

    fn length(&self) -> Duration {
        self.duration.mul_f32((self.to - self.from).abs())
    }

    fn progress(&self, now: Instant) -> f32 {
        let length = self.length();
        if length.is_zero() {
            return 1.0;
        }
        (now.saturating_duration_since(self.since).as_secs_f32() / length.as_secs_f32()).min(1.0)
    }

    fn done(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }

    fn position(&self, now: Instant) -> f32 {
        let t = self.progress(now);
        let eased = if self.to >= self.from {
            1.0 - (1.0 - t).powi(3)
        } else {
            t.powi(3)
        };
        self.from + (self.to - self.from) * eased
    }

    /// Heads for `to` from wherever the slide is now.
    fn retarget(&self, to: f32, now: Instant) -> Self {
        Self::new(self.position(now), to, now, self.duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: Duration = Duration::from_millis(200);

    #[test]
    fn the_surface_takes_the_configured_fractions_of_the_work_area() {
        assert_eq!(
            surface_size(Size::new(2560.0, 1400.0), 1.0, 0.5),
            (2560, 700)
        );
        assert_eq!(
            surface_size(Size::new(1920.0, 1032.0), 0.8, 0.33),
            (1536, 341)
        );
    }

    #[test]
    fn the_surface_is_never_empty() {
        assert_eq!(surface_size(Size::new(0.0, 0.0), 0.5, 0.5), (1, 1));
    }

    #[test]
    fn a_hidden_surface_sits_its_full_height_above_the_work_area() {
        assert_eq!(top_margin(700, 0.0), -700);
        assert_eq!(top_margin(700, 0.5), -350);
        assert_eq!(top_margin(700, 1.0), 0);
        assert_eq!(top_margin(700, 1.5), 0);
    }

    #[test]
    fn screens_map_to_layer_shell_outputs() {
        assert!(matches!(
            output_option(&Monitor::Pointer),
            OutputOption::Active
        ));
        assert!(matches!(
            output_option(&Monitor::Primary),
            OutputOption::Active
        ));
        assert!(matches!(
            output_option(&Monitor::Connector("DP-4".into())),
            OutputOption::OutputName(name) if name == "DP-4"
        ));
    }

    #[test]
    fn a_slide_eases_from_start_to_end() {
        let start = Instant::now();
        let slide = Slide::new(0.0, 1.0, start, FULL);
        assert!(slide.position(start).abs() < f32::EPSILON);
        let halfway = slide.position(start + FULL / 2);
        assert!(
            halfway > 0.5 && halfway < 1.0,
            "ease-out is past half: {halfway}"
        );
        assert!((slide.position(start + FULL) - 1.0).abs() < f32::EPSILON);
        assert!(slide.done(start + FULL));
        assert!(!slide.done(start + FULL / 2));
    }

    #[test]
    fn sliding_out_eases_in() {
        let start = Instant::now();
        let slide = Slide::new(1.0, 0.0, start, FULL);
        let halfway = slide.position(start + FULL / 2);
        assert!(halfway > 0.5, "ease-in has moved less than half: {halfway}");
        assert!(slide.position(start + FULL).abs() < f32::EPSILON);
    }

    #[test]
    fn without_animation_a_slide_ends_at_once() {
        let start = Instant::now();
        let slide = Slide::new(0.0, 1.0, start, Duration::ZERO);
        assert!(slide.done(start));
        assert!((slide.position(start) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn reversing_midway_continues_from_the_current_position_and_takes_less_time() {
        let start = Instant::now();
        let slide = Slide::new(0.0, 1.0, start, FULL);
        let turn = start + FULL / 2;
        let back = slide.retarget(0.0, turn);
        assert!((back.from - slide.position(turn)).abs() < f32::EPSILON);
        assert!(back.length() < FULL);
        assert!(back.done(turn + back.length()));
    }

    fn window() -> dropshade_config::Window {
        dropshade_config::Window {
            animation_duration: 200,
            ..dropshade_config::Window::default()
        }
    }

    #[test]
    fn a_new_drop_down_measures_before_it_shows() {
        let (dropdown, _) = Dropdown::open(&window());
        assert!(dropdown.measuring());
        assert!(!dropdown.showing());
    }

    #[test]
    fn once_placed_it_slides_in_and_stops() {
        let (mut dropdown, _) = Dropdown::open(&window());
        let now = Instant::now();
        let _ = dropdown.placed(Size::new(2000.0, 1000.0), now);
        assert!(!dropdown.measuring());
        assert!(dropdown.showing());
        assert!(dropdown.animating());
        let (_, hidden) = dropdown.tick(now + FULL / 2);
        assert!(!hidden);
        assert!(dropdown.animating());
        let (_, hidden) = dropdown.tick(now + FULL);
        assert!(!hidden);
        assert!(!dropdown.animating(), "the final step was applied");
    }

    #[test]
    fn hiding_finishes_once_it_has_slid_out() {
        let (mut dropdown, _) = Dropdown::open(&window());
        let now = Instant::now();
        let _ = dropdown.placed(Size::new(2000.0, 1000.0), now);
        let shown = now + FULL;
        dropdown.hide(shown);
        assert!(!dropdown.showing());
        assert!(!dropdown.tick(shown + FULL / 2).1);
        assert!(dropdown.tick(shown + FULL).1);
    }

    #[test]
    fn hiding_before_it_is_placed_closes_at_once() {
        let (mut dropdown, _) = Dropdown::open(&window());
        dropdown.hide(Instant::now());
        assert!(dropdown.tick(Instant::now()).1);
    }
}

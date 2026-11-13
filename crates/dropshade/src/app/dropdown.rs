//! The drop-down as a layer-shell surface, for compositors that offer `zwlr_layer_shell_v1`
//! (COSMIC, KDE Plasma, Sway, Hyprland, niri, …). Dropshade places and slides it itself.
//!
//! A new surface first covers the whole work area to learn its size, drawing nothing, then
//! shrinks to the configured fraction and slides in by animating its margin to the top or bottom
//! edge, from beyond that edge to where it sits. Layer-shell offers no surface opacity to fade
//! it with, so "fade" appears and disappears at once, like "none".

use std::time::{Duration, Instant};

use dropshade_config::{Monitor, WindowAlign, WindowAnimation, WindowVerticalAlign};
use iced::{Size, Task, window};
use iced_layershell::actions::{LayerShellCustomAction, LayerShellCustomActionWithId};
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};

use super::Message;
use crate::screens::{self, Screen};

pub const NAMESPACE: &str = "dropshade";

#[derive(Debug)]
pub struct Dropdown {
    pub id: window::Id,
    /// The surface size, once the work area is known.
    size: Option<(u32, u32)>,
    /// The work area the surface measured.
    work_area: Size,
    /// Fills the work area instead of the configured fractions, until it slides out.
    maximized: bool,
    slide: Slide,
    /// The last tick put the surface where the slide ends.
    settled: bool,
    width: f64,
    height: f64,
    align: WindowAlign,
    vertical_align: WindowVerticalAlign,
    animation: WindowAnimation,
}

impl Dropdown {
    /// Creates the surface on the first configured screen that is connected; it slides in once
    /// it knows its size.
    pub fn open(
        window: &dropshade_config::Window,
        screens: Option<&[Screen]>,
    ) -> (Self, Task<Message>) {
        let id = window::Id::unique();
        let duration = match window.animation {
            WindowAnimation::SlideTop | WindowAnimation::SlideBottom => {
                Duration::from_millis(window.animation_duration.into())
            }
            WindowAnimation::Fade | WindowAnimation::None => Duration::ZERO,
        };
        let dropdown = Self {
            id,
            size: None,
            work_area: Size::ZERO,
            maximized: false,
            slide: Slide::new(0.0, 0.0, Instant::now(), duration),
            settled: true,
            width: window.width,
            height: window.height,
            align: window.align,
            vertical_align: window.vertical_align,
            animation: window.animation,
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
            output_option: output_option(&screens::resolve(&window.monitor, screens)),
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
        self.work_area = work_area;
        self.slide = Slide::new(0.0, 1.0, now, self.slide.duration);
        self.settled = false;
        Task::batch([
            self.resize(now),
            layer(
                Some(self.id),
                LayerShellCustomAction::KeyboardInteractivityChange(
                    KeyboardInteractivity::OnDemand,
                ),
            ),
        ])
    }

    /// Switches between filling the work area and the configured size.
    pub fn toggle_maximize(&mut self, now: Instant) -> Task<Message> {
        if self.size.is_none() {
            return Task::none();
        }
        self.maximized = !self.maximized;
        self.resize(now)
    }

    /// Sizes the surface for the work area and places it where the slide is now.
    fn resize(&mut self, now: Instant) -> Task<Message> {
        let (width, height) = if self.maximized {
            (1.0, 1.0)
        } else {
            (self.width, self.height)
        };
        let size = surface_size(self.work_area, width, height);
        self.size = Some(size);
        Task::batch([
            layer(
                Some(self.id),
                LayerShellCustomAction::AnchorSizeChange(
                    anchor(self.align, self.vertical_align, self.animation),
                    size,
                ),
            ),
            self.margin(now),
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
        let place = Placement {
            vertical_align: self.vertical_align,
            animation: self.animation,
            work_height: whole_pixels(self.work_area.height),
            height,
        };
        layer(
            Some(self.id),
            LayerShellCustomAction::MarginChange(place.margins(self.slide.position(now))),
        )
    }
}

fn layer(id: Option<window::Id>, action: LayerShellCustomAction) -> Task<Message> {
    Task::done(Message::Layer(LayerShellCustomActionWithId::new(
        id, action,
    )))
}

/// The side the surface hugs, and the top or bottom edge its slide starts beyond; anchored to
/// neither edge of a pair it is centered between them.
fn anchor(
    align: WindowAlign,
    vertical_align: WindowVerticalAlign,
    animation: WindowAnimation,
) -> Anchor {
    let horizontal = match align {
        WindowAlign::Left => Anchor::Left,
        WindowAlign::Center => Anchor::empty(),
        WindowAlign::Right => Anchor::Right,
    };
    let vertical = match (animation, vertical_align) {
        (WindowAnimation::SlideTop, _)
        | (WindowAnimation::Fade | WindowAnimation::None, WindowVerticalAlign::Top) => Anchor::Top,
        (WindowAnimation::SlideBottom, _)
        | (WindowAnimation::Fade | WindowAnimation::None, WindowVerticalAlign::Bottom) => {
            Anchor::Bottom
        }
        (WindowAnimation::Fade | WindowAnimation::None, WindowVerticalAlign::Center) => {
            Anchor::empty()
        }
    };
    horizontal | vertical
}

/// What decides the surface's vertical margin.
struct Placement {
    vertical_align: WindowVerticalAlign,
    animation: WindowAnimation,
    work_height: u32,
    height: u32,
}

impl Placement {
    /// The (top, right, bottom, left) margins when `shown` (0 to 1) of the slide is done.
    fn margins(&self, shown: f32) -> (i32, i32, i32, i32) {
        let spare = self.work_height.saturating_sub(self.height);
        let above = match self.vertical_align {
            WindowVerticalAlign::Top => 0,
            WindowVerticalAlign::Center => spare / 2,
            WindowVerticalAlign::Bottom => spare,
        };
        match self.animation {
            WindowAnimation::SlideTop => (slid(above, self.height, shown), 0, 0, 0),
            WindowAnimation::SlideBottom => (0, 0, slid(spare - above, self.height, shown), 0),
            WindowAnimation::Fade | WindowAnimation::None => (0, 0, 0, 0),
        }
    }
}

/// Wayland has no primary screen, so "primary" falls back to the compositor's choice.
fn output_option(monitor: &Monitor) -> OutputOption {
    match monitor {
        Monitor::Pointer | Monitor::Primary => OutputOption::Active,
        Monitor::Named(connector) => OutputOption::OutputName(connector.clone()),
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

/// The margin to an edge `gap` pixels from where the surface rests, when `shown` (0 to 1) of
/// the slide is done; hidden, the surface sits its full `height` beyond that edge.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "screen lengths fit in i32"
)]
fn slid(gap: u32, height: u32, shown: f32) -> i32 {
    let travel = f64::from(gap + height) * f64::from(1.0 - shown.clamp(0.0, 1.0));
    gap as i32 - travel.round() as i32
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a screen length fits in u32"
)]
fn whole_pixels(length: f32) -> u32 {
    length.round().max(0.0) as u32
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
    fn a_hidden_surface_sits_its_full_height_beyond_the_edge() {
        assert_eq!(slid(0, 700, 0.0), -700);
        assert_eq!(slid(0, 700, 0.5), -350);
        assert_eq!(slid(0, 700, 1.0), 0);
        assert_eq!(slid(0, 700, 1.5), 0);
        assert_eq!(slid(300, 700, 0.0), -700);
        assert_eq!(slid(300, 700, 1.0), 300);
    }

    fn placement(vertical_align: WindowVerticalAlign, animation: WindowAnimation) -> Placement {
        Placement {
            vertical_align,
            animation,
            work_height: 1000,
            height: 400,
        }
    }

    #[test]
    fn a_slide_ends_where_the_surface_sits() {
        use WindowAnimation as A;
        use WindowVerticalAlign as V;
        assert_eq!(placement(V::Top, A::SlideTop).margins(1.0), (0, 0, 0, 0));
        assert_eq!(
            placement(V::Center, A::SlideTop).margins(1.0),
            (300, 0, 0, 0)
        );
        assert_eq!(
            placement(V::Bottom, A::SlideTop).margins(1.0),
            (600, 0, 0, 0)
        );
        assert_eq!(
            placement(V::Top, A::SlideBottom).margins(1.0),
            (0, 0, 600, 0)
        );
        assert_eq!(
            placement(V::Center, A::SlideBottom).margins(1.0),
            (0, 0, 300, 0)
        );
        assert_eq!(
            placement(V::Bottom, A::SlideBottom).margins(1.0),
            (0, 0, 0, 0)
        );
    }

    #[test]
    fn a_slide_starts_beyond_its_edge() {
        use WindowAnimation as A;
        use WindowVerticalAlign as V;
        for v in [V::Top, V::Center, V::Bottom] {
            assert_eq!(placement(v, A::SlideTop).margins(0.0), (-400, 0, 0, 0));
            assert_eq!(placement(v, A::SlideBottom).margins(0.0), (0, 0, -400, 0));
            assert_eq!(placement(v, A::Fade).margins(0.0), (0, 0, 0, 0));
        }
    }

    #[test]
    fn the_surface_hugs_its_side_and_the_edge_it_slides_from() {
        use WindowAlign as H;
        use WindowAnimation as A;
        use WindowVerticalAlign as V;
        assert_eq!(
            anchor(H::Left, V::Bottom, A::SlideTop),
            Anchor::Top | Anchor::Left
        );
        assert_eq!(anchor(H::Center, V::Center, A::SlideTop), Anchor::Top);
        assert_eq!(
            anchor(H::Right, V::Top, A::SlideBottom),
            Anchor::Bottom | Anchor::Right
        );
        assert_eq!(anchor(H::Left, V::Top, A::Fade), Anchor::Top | Anchor::Left);
        assert_eq!(anchor(H::Center, V::Center, A::Fade), Anchor::empty());
        assert_eq!(
            anchor(H::Right, V::Bottom, A::None),
            Anchor::Bottom | Anchor::Right
        );
    }

    #[test]
    fn a_fade_appears_and_disappears_at_once() {
        let window = dropshade_config::Window {
            animation: WindowAnimation::Fade,
            ..window()
        };
        let (mut dropdown, _) = Dropdown::open(&window, None);
        let now = Instant::now();
        let _ = dropdown.placed(Size::new(2000.0, 1000.0), now);
        assert!(!dropdown.tick(now).1);
        assert!(!dropdown.animating());
        dropdown.hide(now);
        assert!(dropdown.tick(now).1);
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
            output_option(&Monitor::Named("DP-4".into())),
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
        let (dropdown, _) = Dropdown::open(&window(), None);
        assert!(dropdown.measuring());
        assert!(!dropdown.showing());
    }

    #[test]
    fn once_placed_it_slides_in_and_stops() {
        let (mut dropdown, _) = Dropdown::open(&window(), None);
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
        let (mut dropdown, _) = Dropdown::open(&window(), None);
        let now = Instant::now();
        let _ = dropdown.placed(Size::new(2000.0, 1000.0), now);
        let shown = now + FULL;
        dropdown.hide(shown);
        assert!(!dropdown.showing());
        assert!(!dropdown.tick(shown + FULL / 2).1);
        assert!(dropdown.tick(shown + FULL).1);
    }

    #[test]
    fn maximizing_fills_the_work_area_and_toggles_back() {
        let (mut dropdown, _) = Dropdown::open(&window(), None);
        let now = Instant::now();
        let _ = dropdown.placed(Size::new(2000.0, 1000.0), now);
        let configured = dropdown.size;
        let _ = dropdown.toggle_maximize(now);
        assert_eq!(dropdown.size, Some((2000, 1000)));
        let _ = dropdown.toggle_maximize(now);
        assert_eq!(dropdown.size, configured);
    }

    #[test]
    fn maximizing_before_it_is_placed_does_nothing() {
        let (mut dropdown, _) = Dropdown::open(&window(), None);
        let _ = dropdown.toggle_maximize(Instant::now());
        assert!(dropdown.measuring());
        assert!(!dropdown.maximized);
    }

    #[test]
    fn hiding_before_it_is_placed_closes_at_once() {
        let (mut dropdown, _) = Dropdown::open(&window(), None);
        dropdown.hide(Instant::now());
        assert!(dropdown.tick(Instant::now()).1);
    }
}

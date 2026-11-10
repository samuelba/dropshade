//! The iced widget that hosts a [`TerminalView`]. It works like iced's `Canvas`, and additionally
//! enables the input method so IME composition (CJK input, compose sequences) reaches the shell.

use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::input_method::{InputMethod, Preedit, Purpose};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell};
use iced::widget::canvas::Program;
use iced::{
    Element, Event, Length, Pixels, Rectangle, Renderer, Size, Theme, Vector, event, mouse, window,
};

use super::{TerminalView, ViewState};

pub struct TerminalWidget<'a, Message> {
    view: TerminalView<'a, Message>,
    last_interaction: Option<mouse::Interaction>,
}

impl<Message> std::fmt::Debug for TerminalWidget<'_, Message> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalWidget")
            .field("view", &self.view)
            .finish_non_exhaustive()
    }
}

impl<'a, Message> TerminalWidget<'a, Message> {
    pub fn new(view: TerminalView<'a, Message>) -> Self {
        Self {
            view,
            last_interaction: None,
        }
    }

    fn request_input_method(
        &self,
        state: &ViewState,
        bounds: Rectangle,
        shell: &mut Shell<'_, Message>,
    ) {
        if !state.focused || !self.view.input.ime || !self.view.accepts_input {
            return;
        }
        let Some(cursor) = self.view.ime_cursor(bounds) else {
            return;
        };
        let preedit = state.preedit.as_deref().map(|content| Preedit {
            content,
            selection: None,
            text_size: Some(Pixels(self.view.style.font_size)),
        });
        shell.request_input_method(&InputMethod::Enabled {
            cursor,
            purpose: Purpose::Terminal,
            preedit,
        });
    }
}

impl<Message> Widget<Message, Theme, Renderer> for TerminalWidget<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ViewState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(ViewState::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, Length::Fill, Length::Fill)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let is_redraw = matches!(event, Event::Window(window::Event::RedrawRequested(_)));
        let state = tree.state.downcast_mut::<ViewState>();

        if let Some(action) = self.view.update(state, event, bounds, cursor) {
            let (message, redraw, status) = action.into_inner();
            shell.request_redraw_at(redraw);
            if let Some(message) = message {
                shell.publish(message);
            }
            if status == event::Status::Captured {
                shell.capture_event();
            }
        }
        if is_redraw {
            self.request_input_method(tree.state.downcast_ref::<ViewState>(), bounds, shell);
        }

        let interaction = self.mouse_interaction(tree, layout, cursor, viewport, renderer);
        if is_redraw {
            self.last_interaction = Some(interaction);
        } else if self
            .last_interaction
            .is_some_and(|last| last != interaction)
        {
            shell.request_redraw();
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        self.view.mouse_interaction(
            tree.state.downcast_ref::<ViewState>(),
            layout.bounds(),
            cursor,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if bounds.width < 1.0 || bounds.height < 1.0 {
            return;
        }
        let state = tree.state.downcast_ref::<ViewState>();
        renderer.with_translation(Vector::new(bounds.x, bounds.y), |renderer| {
            let geometry = self.view.render(state, renderer, bounds, cursor);
            renderer.draw_geometry(geometry);
        });
    }
}

impl<'a, Message: 'a> From<TerminalWidget<'a, Message>> for Element<'a, Message> {
    fn from(widget: TerminalWidget<'a, Message>) -> Self {
        Element::new(widget)
    }
}

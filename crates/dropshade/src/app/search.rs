//! The scrollback search bar (Ctrl+Shift+F).

use dropshade_term::SearchDirection;
use iced::widget::{Id, Space, button, container, operation, row, text, text_input};
use iced::{Element, Fill, Length, Task};

use super::{App, Message};

const INPUT_ID: &str = "scrollback-search";
const INPUT_WIDTH: f32 = 280.0;
const TEXT_SIZE: f32 = 13.0;

#[derive(Debug, Default)]
pub(super) struct SearchBar {
    query: String,
    /// Whether the last search found something; `None` before the first search.
    found: Option<bool>,
    /// Opened in vi mode: matches move the vi cursor, and Enter returns to it.
    vi: bool,
}

#[derive(Debug, Clone)]
pub enum SearchMessage {
    Input(String),
    /// Enter in the query field.
    Submit,
    Older,
    Newer,
    Close,
}

impl App {
    pub(super) fn open_search(&mut self) -> Task<Message> {
        let vi = self
            .focused_session()
            .is_some_and(|session| session.terminal.vi_mode());
        let bar = self.search.get_or_insert_with(SearchBar::default);
        if vi && !bar.vi {
            *bar = SearchBar {
                vi,
                ..SearchBar::default()
            };
        }
        bar.vi = vi;
        let input = Id::new(INPUT_ID);
        Task::batch([
            operation::focus(input.clone()),
            operation::select_all(input),
        ])
    }

    pub(super) fn close_search(&mut self) {
        self.search = None;
        if let Some(session) = self.focused_session_mut() {
            session.terminal.clear_search();
        }
    }

    pub(super) fn search_message(&mut self, message: SearchMessage) -> Task<Message> {
        let vi = self.search.as_ref().is_some_and(|bar| bar.vi);
        if matches!(message, SearchMessage::Close)
            || (vi && matches!(message, SearchMessage::Submit))
        {
            // In vi mode the query stays, for `n` and `N`.
            self.close_search();
            return Task::none();
        }
        // The tabs and the bar are borrowed apart, so not through `focused_session_mut`.
        let session = self
            .tabs
            .active_mut()
            .and_then(|tab| tab.content.focused_content_mut());
        let (Some(bar), Some(session)) = (self.search.as_mut(), session) else {
            return Task::none();
        };
        let terminal = &mut session.terminal;
        let found = match message {
            SearchMessage::Input(query) => {
                let found = terminal.search(&query);
                bar.found = (!query.is_empty()).then_some(found);
                bar.query = query;
                return Task::none();
            }
            SearchMessage::Older | SearchMessage::Submit => {
                terminal.search_next(SearchDirection::Older)
            }
            SearchMessage::Newer => terminal.search_next(SearchDirection::Newer),
            SearchMessage::Close => false,
        };
        if !bar.query.is_empty() {
            bar.found = Some(found);
        }
        Task::none()
    }

    pub(super) fn search_bar(&self) -> Option<Element<'_, Message>> {
        let bar = self.search.as_ref()?;
        let colors = self.settings.colors;
        let icon = |label: &'static str, message: SearchMessage| {
            button(text(label).size(TEXT_SIZE + 1.0))
                .padding([2, 8])
                .style(move |_, status| colors.ghost_button(colors.text, status))
                .on_press(Message::Search(message))
        };
        let status = match bar.found {
            Some(false) => "No matches",
            Some(true) | None => "",
        };
        let content = row![
            text_input("Find in scrollback", &bar.query)
                .id(Id::new(INPUT_ID))
                .on_input(|query| Message::Search(SearchMessage::Input(query)))
                .on_submit(Message::Search(SearchMessage::Submit))
                .size(TEXT_SIZE)
                .padding([4, 8])
                .width(Length::Fixed(INPUT_WIDTH)),
            icon("↑", SearchMessage::Older),
            icon("↓", SearchMessage::Newer),
            text(status).size(TEXT_SIZE).color(colors.muted),
            Space::new().width(Fill),
            icon("×", SearchMessage::Close),
        ]
        .spacing(6)
        .align_y(iced::Center);
        Some(
            container(content)
                .padding([4, 10])
                .width(Fill)
                .style(move |_| container::Style::default().background(colors.surface))
                .into(),
        )
    }
}

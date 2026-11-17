//! The card in a corner of the terminal that tells about a newer release. It updates installs
//! made by `install.sh` in place and links other installs to the release page. It never takes
//! the keyboard: typing keeps going to the terminal while it shows.

use dropshade_term::colors::to_iced;
use iced::widget::{Space, button, column, container, row, text};
use iced::{Color, Element, Fill, Font, Task, font};

use super::update::open_link;
use super::{App, Message};
use crate::release::{self, InstallKind, Installed, Release, Restart};
use crate::tabs::BarPosition;
use crate::theme::UiColors;

const CARD_WIDTH: f32 = 340.0;
const CARD_MARGIN: f32 = 12.0;
const TEXT_SIZE: f32 = 13.0;
const RUNNING: &str = env!("CARGO_PKG_VERSION");
const BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..Font::DEFAULT
};

/// What the card shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Card {
    #[default]
    Hidden,
    Available,
    Updating,
    /// The new binary is in place and runs after a restart.
    Installed(Installed),
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum ReleaseMessage {
    /// Look for a newer release, at start and once a day.
    Check,
    Checked(Result<Option<Release>, String>),
    /// Update a script install; open the release page for other installs.
    Update,
    Updated(Result<Installed, String>),
    /// Open the release page: its notes, and the packages for other installs.
    OpenRelease,
    Skip,
    /// Hide the card until the next start or check.
    Dismiss,
    Restart,
}

impl App {
    /// Starts looking for a newer release, unless the config turns that off.
    pub(super) fn check_release(&self) -> Task<Message> {
        if self.config.updates.check {
            Task::perform(release::check(), |result| {
                Message::Release(ReleaseMessage::Checked(result))
            })
        } else {
            Task::none()
        }
    }

    pub(super) fn release_message(&mut self, message: ReleaseMessage) -> Task<Message> {
        match message {
            ReleaseMessage::Check => return self.check_release(),
            ReleaseMessage::Checked(result) => self.release_checked(result),
            ReleaseMessage::Update => return self.start_update(),
            ReleaseMessage::Updated(Ok(installed)) => {
                tracing::info!(?installed, "installed the new release");
                if let (InstallKind::Script { installed, .. }, Some(release)) =
                    (&mut self.install, &self.release)
                {
                    *installed = Some(release.version.clone());
                }
                self.updated = Some(installed);
                self.card = Card::Installed(installed);
            }
            ReleaseMessage::Updated(Err(error)) => {
                tracing::warn!(%error, "the update failed");
                self.card = Card::Failed(error);
            }
            ReleaseMessage::OpenRelease => {
                if let Some(release) = &self.release {
                    open_link(&release.url);
                }
            }
            ReleaseMessage::Skip => {
                if let (Some(release), Some(path)) = (&self.release, release::skipped_path()) {
                    release::skip(&path, &release.version);
                }
                self.card = Card::Hidden;
            }
            ReleaseMessage::Dismiss => self.card = Card::Hidden,
            ReleaseMessage::Restart => return self.restart(),
        }
        Task::none()
    }

    fn release_checked(&mut self, result: Result<Option<Release>, String>) {
        let found = match result {
            Ok(Some(found)) => found,
            Ok(None) => return tracing::debug!("this is the latest release"),
            Err(error) => return tracing::warn!(%error, "cannot check for a newer release"),
        };
        tracing::info!(version = %found.version, "a newer release is available");
        let skipped =
            release::skipped_path().is_some_and(|path| release::is_skipped(&path, &found.version));
        self.card = card_after_check(&self.card, &self.install, self.updated, &found, skipped);
        self.release = Some(found);
    }

    /// Updates a script install; other installs get the release page, where their package is.
    fn start_update(&mut self) -> Task<Message> {
        let Some(release) = &self.release else {
            return Task::none();
        };
        if !matches!(self.install, InstallKind::Script { .. }) {
            open_link(&release.url);
            return Task::none();
        }
        if matches!(self.card, Card::Updating | Card::Installed(_)) {
            return Task::none();
        }
        self.card = Card::Updating;
        Task::perform(release::self_update(release.version.clone()), |result| {
            Message::Release(ReleaseMessage::Updated(result))
        })
    }

    /// Saves the tabs and quits; `main` then starts the new binary in this process.
    fn restart(&mut self) -> Task<Message> {
        let InstallKind::Script { binary, .. } = &self.install else {
            return Task::none();
        };
        release::request_restart(Restart {
            binary: binary.clone(),
            config: self.config_path.clone(),
        });
        self.save_session_on_exit();
        iced::exit()
    }

    /// The button About shows for a newer release, if there is one.
    pub(super) fn release_action(&self) -> Option<(String, ReleaseMessage)> {
        let release = self.release.as_ref()?;
        Some(match (&self.install, &self.card) {
            (_, Card::Installed(_)) => (
                format!("Version {} is installed", release.version),
                ReleaseMessage::OpenRelease,
            ),
            (InstallKind::Script { .. }, _) => (
                format!("Update to version {}", release.version),
                ReleaseMessage::Update,
            ),
            (InstallKind::Other, _) => (
                format!("Download version {}", release.version),
                ReleaseMessage::OpenRelease,
            ),
        })
    }

    pub(super) fn release_card(&self) -> Option<Element<'_, Message>> {
        let release = self.release.as_ref()?;
        let colors = self.settings.colors;
        let (title, body, buttons) = self.card_parts(release)?;
        let mut header = row![
            text(title).size(TEXT_SIZE + 1.0).font(BOLD),
            Space::new().width(Fill)
        ];
        if self.card != Card::Updating {
            header = header.push(
                button(text("×").size(16))
                    .padding([0, 6])
                    .style(move |_, status| colors.ghost_button(colors.muted, status))
                    .on_press(Message::Release(ReleaseMessage::Dismiss)),
            );
        }
        let card = container(column![header, body].push(buttons).spacing(8))
            .padding(14)
            .width(CARD_WIDTH)
            .style(move |_| colors.surface_style());
        // In the corner away from the tab bar, so no tab hides under it.
        let placed = container(self.modal(card))
            .padding(CARD_MARGIN)
            .width(Fill)
            .height(Fill)
            .align_right(Fill);
        Some(if self.settings.bar == BarPosition::Bottom {
            placed.align_top(Fill).into()
        } else {
            placed.align_bottom(Fill).into()
        })
    }

    /// The card's title, text and buttons in its current state.
    fn card_parts(&self, release: &Release) -> Option<CardParts<'_>> {
        let colors = self.settings.colors;
        let primary = |label: String, message| primary_button(colors, label, message);
        let secondary = |label, message| secondary_button(colors, label, message);
        let notes = || secondary("Release notes", ReleaseMessage::OpenRelease);
        let later = || secondary("Later", ReleaseMessage::Dismiss);
        let version = &release.version;
        Some(match &self.card {
            Card::Hidden => return None,
            Card::Available => {
                let action = if matches!(self.install, InstallKind::Script { .. }) {
                    primary(format!("Update to {version}"), ReleaseMessage::Update)
                } else {
                    primary("Download".to_owned(), ReleaseMessage::OpenRelease)
                };
                let skip = secondary("Skip this version", ReleaseMessage::Skip);
                (
                    format!("Dropshade {version} is available"),
                    paragraph(format!("You have {RUNNING}."), colors.muted),
                    Some(row![action, notes(), skip].spacing(4).into()),
                )
            }
            Card::Updating => (
                format!("Updating to {version}…"),
                paragraph(
                    "Downloading and installing it. You can keep working.".to_owned(),
                    colors.muted,
                ),
                None,
            ),
            Card::Installed(Installed {
                extension_changed: true,
            }) => (
                format!("Dropshade {version} is installed"),
                paragraph(
                    "So is a new GNOME Shell extension. Log out and back in to use both."
                        .to_owned(),
                    colors.text,
                ),
                Some(row![notes(), later()].spacing(4).into()),
            ),
            Card::Installed(_) => {
                let restart = primary("Restart now".to_owned(), ReleaseMessage::Restart);
                (
                    format!("Dropshade {version} is installed"),
                    self.restart_hint(),
                    Some(row![restart, notes(), later()].spacing(4).into()),
                )
            }
            Card::Failed(error) => (
                "The update failed".to_owned(),
                paragraph(error.clone(), colors.text),
                Some(row![notes()].into()),
            ),
        })
    }

    /// How to restart, with a warning that it ends what runs in the shells.
    fn restart_hint(&self) -> Element<'_, Message> {
        let warning = to_iced(self.settings.style.palette.normal[3], 1.0);
        column![
            paragraph(
                "Restart Dropshade to use it: click Restart now, or run `pkill dropshade` and \
                 open it again."
                    .to_owned(),
                self.settings.colors.text,
            ),
            row![
                text("⚠").size(TEXT_SIZE).color(warning).font(BOLD),
                paragraph(
                    "Restarting closes your running shells and what runs in them. Your tabs and \
                     their directories come back."
                        .to_owned(),
                    warning,
                ),
            ]
            .spacing(6),
        ]
        .spacing(6)
        .into()
    }
}

type CardParts<'a> = (String, Element<'a, Message>, Option<Element<'a, Message>>);

fn primary_button<'a>(
    colors: UiColors,
    label: String,
    message: ReleaseMessage,
) -> Element<'a, Message> {
    button(text(label).size(TEXT_SIZE))
        .padding([6, 12])
        .style(move |_, status| colors.accent_button(status))
        .on_press(Message::Release(message))
        .into()
}

fn secondary_button<'a>(
    colors: UiColors,
    label: &'static str,
    message: ReleaseMessage,
) -> Element<'a, Message> {
    button(text(label).size(TEXT_SIZE))
        .padding([6, 10])
        .style(move |_, status| colors.ghost_button(colors.accent, status))
        .on_press(Message::Release(message))
        .into()
}

fn paragraph<'a>(content: String, color: Color) -> Element<'a, Message> {
    text(content).size(TEXT_SIZE).color(color).into()
}

/// What the card shows once a check found `found`, newer than the running build. `updated` is
/// what an update in this run changed.
fn card_after_check(
    current: &Card,
    install: &InstallKind,
    updated: Option<Installed>,
    found: &Release,
    skipped: bool,
) -> Card {
    // An update is under way or done; a later check changes nothing about that.
    if matches!(current, Card::Updating | Card::Installed(_)) {
        return current.clone();
    }
    // The script already put this version in place, in this run or by hand: only a restart is
    // missing.
    if let InstallKind::Script {
        installed: Some(installed),
        ..
    } = install
        && *installed >= found.version
    {
        return Card::Installed(updated.unwrap_or(Installed {
            extension_changed: false,
        }));
    }
    if skipped {
        Card::Hidden
    } else {
        Card::Available
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use semver::Version;

    use super::*;

    const NOT_SKIPPED: bool = false;

    fn found(version: &str) -> Release {
        Release {
            version: Version::parse(version).expect("version"),
            url: String::new(),
        }
    }

    fn script(installed: &str) -> InstallKind {
        InstallKind::Script {
            binary: PathBuf::from("/home/me/.local/bin/dropshade"),
            installed: Version::parse(installed).ok(),
        }
    }

    #[test]
    fn a_newer_release_shows_the_card() {
        let card = card_after_check(
            &Card::Hidden,
            &InstallKind::Other,
            None,
            &found("0.12.0"),
            NOT_SKIPPED,
        );
        assert_eq!(card, Card::Available);
    }

    #[test]
    fn a_skipped_release_stays_hidden() {
        let card = card_after_check(
            &Card::Hidden,
            &script("0.11.0"),
            None,
            &found("0.12.0"),
            true,
        );
        assert_eq!(card, Card::Hidden);
    }

    #[test]
    fn a_dismissed_card_comes_back_with_the_next_check() {
        let card = card_after_check(
            &Card::Hidden,
            &script("0.11.0"),
            None,
            &found("0.12.0"),
            NOT_SKIPPED,
        );
        assert_eq!(card, Card::Available);
    }

    #[test]
    fn an_installed_release_only_asks_for_a_restart() {
        let card = card_after_check(
            &Card::Hidden,
            &script("0.12.0"),
            None,
            &found("0.12.0"),
            NOT_SKIPPED,
        );
        assert!(matches!(card, Card::Installed(_)), "{card:?}");
    }

    #[test]
    fn an_update_under_way_or_done_is_kept() {
        let card = card_after_check(
            &Card::Updating,
            &script("0.11.0"),
            None,
            &found("0.12.0"),
            NOT_SKIPPED,
        );
        assert_eq!(card, Card::Updating);
        let installed = Card::Installed(Installed {
            extension_changed: true,
        });
        let card = card_after_check(
            &installed,
            &script("0.12.0"),
            None,
            &found("0.12.0"),
            NOT_SKIPPED,
        );
        assert_eq!(card, installed);
    }

    #[test]
    fn a_dismissed_restart_hint_keeps_what_the_update_changed() {
        let updated = Installed {
            extension_changed: true,
        };
        let card = card_after_check(
            &Card::Hidden,
            &script("0.12.0"),
            Some(updated),
            &found("0.12.0"),
            NOT_SKIPPED,
        );
        assert_eq!(card, Card::Installed(updated));
    }

    #[test]
    fn a_failed_update_is_offered_again() {
        let failed = Card::Failed("no network".into());
        let card = card_after_check(
            &failed,
            &script("0.11.0"),
            None,
            &found("0.12.0"),
            NOT_SKIPPED,
        );
        assert_eq!(card, Card::Available);
    }
}

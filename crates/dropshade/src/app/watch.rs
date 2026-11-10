//! Watches the configuration directory (including themes) and reports changes.

use std::path::{Path, PathBuf};

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::{Subscription, stream};
use notify::{EventKind, RecursiveMode, Watcher};

use super::Message;

pub fn config_changes(config_path: PathBuf) -> Subscription<Message> {
    Subscription::run_with(config_path, watch)
}

#[expect(
    clippy::ptr_arg,
    reason = "Subscription::run_with passes the data by reference"
)]
fn watch(config_path: &PathBuf) -> impl Stream<Item = Message> + use<> {
    let config_path = config_path.clone();
    stream::channel(8, async move |mut output: mpsc::Sender<Message>| {
        let Some(directory) = config_path.parent().map(Path::to_path_buf) else {
            return;
        };
        if let Err(error) = std::fs::create_dir_all(&directory) {
            tracing::warn!(%error, dir = %directory.display(), "cannot create the config directory");
            return;
        }
        let (sender, mut changes) = mpsc::unbounded();
        let watcher =
            notify::recommended_watcher(
                move |result: notify::Result<notify::Event>| match result {
                    Ok(event) if is_relevant(&event) => {
                        if sender.unbounded_send(()).is_err() {
                            tracing::trace!("config change dropped: the app is shutting down");
                        }
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "config watcher error"),
                },
            );
        let mut watcher = match watcher {
            Ok(watcher) => watcher,
            Err(error) => {
                tracing::warn!(%error, "cannot watch the config directory; changes need a restart");
                return;
            }
        };
        if let Err(error) = watcher.watch(&directory, RecursiveMode::Recursive) {
            tracing::warn!(%error, dir = %directory.display(), "cannot watch the config directory");
            return;
        }
        while changes.next().await.is_some() {
            if output.send(Message::ConfigChanged).await.is_err() {
                break;
            }
        }
    })
}

fn is_relevant(event: &notify::Event) -> bool {
    let changed = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    );
    changed
        && event
            .paths
            .iter()
            .any(|path| path.extension().is_some_and(|ext| ext == "toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind};

    fn event(kind: EventKind, path: &str) -> notify::Event {
        notify::Event::new(kind).add_path(PathBuf::from(path))
    }

    #[test]
    fn only_toml_changes_matter() {
        assert!(is_relevant(&event(
            EventKind::Modify(ModifyKind::Any),
            "/c/dropshade/config.toml"
        )));
        assert!(is_relevant(&event(
            EventKind::Create(CreateKind::File),
            "/c/dropshade/themes/x.toml"
        )));
        assert!(!is_relevant(&event(
            EventKind::Modify(ModifyKind::Any),
            "/c/dropshade/config.toml.swp"
        )));
        assert!(!is_relevant(&event(
            EventKind::Access(notify::event::AccessKind::Any),
            "/c/config.toml"
        )));
    }
}

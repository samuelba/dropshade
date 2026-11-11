//! A desktop notification announcing a release.

use super::Release;
use crate::app::APP_NAME;
use crate::notifications::{self, DEFAULT_ACTION, Notification};

const DOWNLOAD_ACTION: &str = "download";

/// Shows the notification and waits for it; `true` when the user asked to download.
pub async fn announce(release: Release) -> bool {
    let summary = format!("{APP_NAME} {} is available", release.version);
    let body = format!(
        "You are running {}. Download the new version from GitHub.",
        env!("CARGO_PKG_VERSION")
    );
    let notification = Notification {
        summary: &summary,
        body: &body,
        actions: &[DEFAULT_ACTION, "Download", DOWNLOAD_ACTION, "Download"],
    };
    match notifications::show_and_wait(notification).await {
        Ok(action) => matches!(action.as_deref(), Some(DEFAULT_ACTION | DOWNLOAD_ACTION)),
        Err(error) => {
            tracing::warn!(%error, "cannot show the update notification");
            false
        }
    }
}

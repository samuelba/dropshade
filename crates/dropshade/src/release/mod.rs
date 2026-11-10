//! Looking up the latest release on GitHub and telling the user when it is newer.

pub mod notify;

use std::path::{Path, PathBuf};
use std::time::Duration;

use semver::Version;
use serde::Deserialize;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const GITHUB: &str = "https://github.com/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const NOTIFIED_FILE: &str = "update-notified";

/// A published release that is newer than the running build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    /// The release page, where the packages are downloaded.
    pub url: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CheckError {
    #[error("the repository {0} is not on GitHub")]
    NotGitHub(&'static str),
    #[error("request failed: {0}")]
    Request(#[from] ureq::Error),
    #[error("the release tag {0:?} is not a version")]
    BadTag(String),
    #[error("the release page {0:?} is not in this repository")]
    BadUrl(String),
    #[error("the check was cancelled")]
    Cancelled,
}

/// The fields of GitHub's release object that matter here.
#[derive(Debug, Deserialize)]
struct LatestRelease {
    tag_name: String,
    html_url: String,
}

/// Asks GitHub for the latest release; `None` when the running build is up to date.
pub async fn check() -> Result<Option<Release>, String> {
    tokio::task::spawn_blocking(fetch_newer)
        .await
        .map_err(|_| CheckError::Cancelled)
        .and_then(|result| result)
        .map_err(|error| error.to_string())
}

fn fetch_newer() -> Result<Option<Release>, CheckError> {
    let url = latest_release_api(REPOSITORY).ok_or(CheckError::NotGitHub(REPOSITORY))?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .into();
    let latest: LatestRelease = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header(
            "User-Agent",
            concat!("dropshade/", env!("CARGO_PKG_VERSION")),
        )
        .call()?
        .body_mut()
        .read_json()?;
    newer_than(CURRENT_VERSION, latest)
}

/// The GitHub API endpoint for a `https://github.com/OWNER/REPO` repository.
fn latest_release_api(repository: &str) -> Option<String> {
    let path = repository.strip_prefix(GITHUB)?.trim_end_matches('/');
    let (owner, repo) = path.split_once('/')?;
    let valid = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (valid(owner) && valid(repo))
        .then(|| format!("https://api.github.com/repos/{owner}/{repo}/releases/latest"))
}

fn newer_than(current: &str, latest: LatestRelease) -> Result<Option<Release>, CheckError> {
    let version = parse_tag(&latest.tag_name)?;
    if !latest
        .html_url
        .starts_with(&format!("{}/releases/", REPOSITORY.trim_end_matches('/')))
    {
        return Err(CheckError::BadUrl(latest.html_url));
    }
    let current = Version::parse(current).map_err(|_| CheckError::BadTag(current.to_owned()))?;
    Ok((version > current).then(|| Release {
        version: version.to_string(),
        url: latest.html_url,
    }))
}

fn parse_tag(tag: &str) -> Result<Version, CheckError> {
    let bare = tag.strip_prefix('v').unwrap_or(tag);
    Version::parse(bare).map_err(|_| CheckError::BadTag(tag.to_owned()))
}

/// Where the last announced version is kept, so each release is announced once.
pub fn notified_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "samuelba", "dropshade")
        .and_then(|dirs| dirs.state_dir().map(|dir| dir.join(NOTIFIED_FILE)))
}

/// Whether `release` still needs announcing; records it as announced when it does.
pub fn first_announcement(path: &Path, release: &Release) -> bool {
    let announced =
        std::fs::read_to_string(path).is_ok_and(|previous| previous.trim() == release.version);
    if announced {
        return false;
    }
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, &release.version));
    if let Err(error) = written {
        tracing::warn!(path = %path.display(), %error, "cannot remember the announced release");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latest(tag: &str) -> LatestRelease {
        LatestRelease {
            tag_name: tag.into(),
            html_url: format!("{REPOSITORY}/releases/tag/{tag}"),
        }
    }

    #[test]
    fn a_higher_release_is_offered() {
        let release = newer_than("0.2.0", latest("v0.3.0"))
            .expect("valid")
            .expect("newer");
        assert_eq!(release.version, "0.3.0");
        assert_eq!(release.url, format!("{REPOSITORY}/releases/tag/v0.3.0"));
    }

    #[test]
    fn the_same_or_an_older_release_is_not_offered() {
        assert_eq!(newer_than("0.3.0", latest("v0.3.0")).expect("valid"), None);
        assert_eq!(newer_than("0.3.0", latest("v0.2.9")).expect("valid"), None);
    }

    #[test]
    fn versions_compare_numerically() {
        let release = newer_than("0.9.0", latest("v0.10.0")).expect("valid");
        assert!(release.is_some());
    }

    #[test]
    fn tags_without_the_v_prefix_are_understood() {
        assert!(
            newer_than("0.2.0", latest("0.3.0"))
                .expect("valid")
                .is_some()
        );
    }

    #[test]
    fn a_tag_that_is_not_a_version_is_an_error() {
        assert!(matches!(
            newer_than("0.2.0", latest("nightly")),
            Err(CheckError::BadTag(_))
        ));
    }

    #[test]
    fn a_release_page_outside_the_repository_is_rejected() {
        let elsewhere = LatestRelease {
            tag_name: "v9.0.0".into(),
            html_url: "https://example.com/releases/tag/v9.0.0".into(),
        };
        assert!(matches!(
            newer_than("0.2.0", elsewhere),
            Err(CheckError::BadUrl(_))
        ));
    }

    #[test]
    fn the_api_url_is_derived_from_the_repository() {
        assert_eq!(
            latest_release_api("https://github.com/owner/repo").as_deref(),
            Some("https://api.github.com/repos/owner/repo/releases/latest")
        );
        assert_eq!(
            latest_release_api("https://github.com/owner/repo/").as_deref(),
            Some("https://api.github.com/repos/owner/repo/releases/latest")
        );
        assert!(latest_release_api(REPOSITORY).is_some());
    }

    #[test]
    fn non_github_or_odd_repositories_have_no_api_url() {
        assert_eq!(latest_release_api("https://gitlab.com/owner/repo"), None);
        assert_eq!(latest_release_api("https://github.com/owner"), None);
        assert_eq!(latest_release_api("https://github.com/owner/re?po"), None);
    }

    #[test]
    fn the_running_version_parses() {
        assert!(Version::parse(CURRENT_VERSION).is_ok());
    }

    #[test]
    #[ignore = "talks to the GitHub API"]
    fn the_latest_release_can_be_fetched() {
        let release = fetch_newer().expect("the check succeeds");
        if let Some(release) = release {
            assert!(release.url.starts_with(REPOSITORY));
        }
    }

    #[test]
    fn each_release_is_announced_once() {
        let dir = std::env::temp_dir().join(format!("dropshade-release-{}", std::process::id()));
        let path = dir.join("state").join(NOTIFIED_FILE);
        let release = |version: &str| Release {
            version: version.into(),
            url: String::new(),
        };
        assert!(first_announcement(&path, &release("0.3.0")));
        assert!(!first_announcement(&path, &release("0.3.0")));
        assert!(first_announcement(&path, &release("0.4.0")));
        std::fs::remove_dir_all(&dir).expect("clean up");
    }
}

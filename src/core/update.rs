//! Self-update (plan §13): which GitHub release to offer, and when to ask.
//!
//! Windows-free and host-testable. The Windows side (`win/update.rs`) does
//! the HTTP, hashing and file swap. A release is offered when its tag is a
//! newer version than the running build and it carries both
//! `smolcast-<version>-x86_64.exe` and the matching `.sha256` file. The
//! checksum catches a corrupt or truncated download; it is not a signature
//! (builds are not signed yet).

use std::cmp::Ordering;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The repository whose releases feed updates.
pub const REPOSITORY: &str = "Archetypically/smolcast";
/// Newest non-draft, non-prerelease release.
pub const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/Archetypically/smolcast/releases/latest";
/// The repository's release pages; release notes open only under it.
pub const RELEASES_URL: &str = "https://github.com/Archetypically/smolcast/releases/";
/// Where this repository's release assets are served from (GitHub then
/// redirects to its download host). Assets anywhere else are refused.
pub const ASSET_URL_PREFIX: &str = "https://github.com/Archetypically/smolcast/releases/download/";

/// How often an automatic check runs, and how soon one is retried after a
/// failure.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
pub const RETRY_INTERVAL: Duration = Duration::from_secs(2 * 60 * 60);
/// The first automatic check waits this long after startup, out of the
/// sign-in rush.
pub const STARTUP_DELAY: Duration = Duration::from_secs(30);

/// Largest release JSON and executable accepted, so a bad response cannot
/// fill memory or disk.
pub const MAX_RELEASE_JSON_BYTES: u64 = 1024 * 1024;
pub const MAX_EXE_BYTES: u64 = 64 * 1024 * 1024;

/// A `MAJOR.MINOR.PATCH[-PRERELEASE]` version, ordered by semver precedence.
/// Build metadata (`+…`) is not accepted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Dot-separated prerelease identifiers; empty for a release.
    pub pre: Vec<String>,
}

impl Version {
    /// Parse `1.2.3`, `v1.2.3` or `1.2.3-beta.1`; anything else is `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let mut parts = core.split('.');
        let mut number = || -> Option<u64> {
            let part = parts.next()?;
            // No leading zeros, no signs: `01` and `+1` are not versions.
            let canonical = part == "0" || (!part.starts_with('0') && !part.is_empty());
            (canonical && part.bytes().all(|b| b.is_ascii_digit()))
                .then(|| part.parse().ok())
                .flatten()
        };
        let (major, minor, patch) = (number()?, number()?, number()?);
        if parts.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => Vec::new(),
            Some(pre) => {
                let ids: Vec<String> = pre.split('.').map(str::to_owned).collect();
                let valid = |id: &String| {
                    !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                };
                if !ids.iter().all(valid) {
                    return None;
                }
                ids
            }
        };
        Some(Self {
            major,
            minor,
            patch,
            pre,
        })
    }

    /// The running build's version.
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("Cargo.toml holds a semver version")
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                // A prerelease sorts below the release it leads to.
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => compare_pre(&self.pre, &other.pre),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Semver rule: numeric identifiers compare as numbers and below
/// alphanumeric ones; a shorter list that is a prefix sorts first.
fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let order = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

/// The fields of GitHub's release JSON that updates read.
#[derive(Deserialize)]
struct ReleaseJson {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<AssetJson>,
}

#[derive(Deserialize)]
struct AssetJson {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// A release asset to download.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Asset {
    pub url: String,
    pub size: u64,
}

/// A newer release this build can install.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Update {
    pub version: Version,
    /// The release's page on GitHub, for its notes.
    pub page_url: String,
    pub exe: Asset,
    pub checksum: Asset,
}

/// The executable's asset name for `version`.
pub fn exe_asset_name(version: &Version) -> String {
    format!("smolcast-{version}-x86_64.exe")
}

/// Read GitHub's latest-release JSON. `Ok(None)` when it is not newer than
/// `current`; an error when it is newer but cannot be installed.
pub fn newer_release(json: &[u8], current: &Version) -> Result<Option<Update>, String> {
    let release: ReleaseJson =
        serde_json::from_slice(json).map_err(|e| format!("unreadable release data ({e})"))?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let Some(version) = Version::parse(&release.tag_name) else {
        return Err(format!(
            "release tag {:?} is not a version",
            release.tag_name
        ));
    };
    if version <= *current {
        return Ok(None);
    }
    let exe_name = exe_asset_name(&version);
    let checksum_name = format!("{exe_name}.sha256");
    let find = |name: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == name && a.browser_download_url.starts_with(ASSET_URL_PREFIX))
            .map(|a| Asset {
                url: a.browser_download_url.clone(),
                size: a.size,
            })
    };
    let (Some(exe), Some(checksum)) = (find(&exe_name), find(&checksum_name)) else {
        return Err(format!(
            "smolcast {version} was released without {exe_name} and its .sha256"
        ));
    };
    if exe.size == 0 || exe.size > MAX_EXE_BYTES {
        return Err(format!("{exe_name} has an unexpected size"));
    }
    // The notes link is opened with the shell: only this repository's pages.
    let page_url = if release.html_url.starts_with(RELEASES_URL) {
        release.html_url
    } else {
        RELEASES_URL.to_owned()
    };
    Ok(Some(Update {
        version,
        page_url,
        exe,
        checksum,
    }))
}

/// The SHA-256 named by a `.sha256` file: 64 hex digits, optionally
/// followed by whitespace and a file name (`sha256sum` format).
pub fn parse_checksum(text: &str) -> Option<[u8; 32]> {
    let hex = text.split_whitespace().next()?;
    if hex.len() != 64 {
        return None;
    }
    let mut digest = [0u8; 32];
    for (i, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(digest)
}

/// What the updater remembers between runs, in `%LOCALAPPDATA%`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateState {
    /// When the last automatic check finished (Unix ms), successful or not.
    #[serde(default)]
    pub last_check_ms: i64,
    /// Whether that check failed, so the next one comes sooner.
    #[serde(default)]
    pub last_check_failed: bool,
    /// A version the user chose to skip; automatic checks do not offer it.
    #[serde(default)]
    pub skipped: Option<String>,
}

impl UpdateState {
    /// Decode the state file; anything unreadable starts over.
    pub fn decode(bytes: &[u8]) -> Self {
        serde_json::from_slice(bytes).unwrap_or_default()
    }

    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(self).expect("plain struct serializes")
    }

    /// How long until the next automatic check is due, at `now_ms`. Never
    /// sooner than `STARTUP_DELAY` from `started_ms`.
    pub fn next_check_in(&self, now_ms: i64, started_ms: i64) -> Duration {
        let interval = if self.last_check_failed {
            RETRY_INTERVAL
        } else {
            CHECK_INTERVAL
        };
        let due = self
            .last_check_ms
            .saturating_add(interval.as_millis() as i64)
            .max(started_ms.saturating_add(STARTUP_DELAY.as_millis() as i64));
        Duration::from_millis(due.saturating_sub(now_ms).max(0) as u64)
    }

    /// Whether an automatic check should offer `version`.
    pub fn offers(&self, version: &Version) -> bool {
        self.skipped.as_deref() != Some(version.to_string().as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn parses_only_semver_versions() {
        assert_eq!(v("v1.2.3"), v("1.2.3"));
        assert_eq!(v("0.10.0-beta.2").pre, vec!["beta", "2"]);
        assert_eq!(v("1.2.3-rc.1").to_string(), "1.2.3-rc.1");
        for bad in [
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.2.x",
            "",
            "v",
            "1.2.3-",
            "1.2.3+b",
            "1.2.3-a..b",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn orders_by_semver_precedence() {
        let ordered = [
            "0.9.9",
            "0.10.0-alpha",
            "0.10.0-alpha.1",
            "0.10.0-alpha.beta",
            "0.10.0-beta.2",
            "0.10.0-beta.11",
            "0.10.0",
            "0.10.1",
            "1.0.0",
        ];
        for pair in ordered.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
    }

    fn release(tag: &str, assets: &[(&str, u64)]) -> Vec<u8> {
        let assets: Vec<serde_json::Value> = assets
            .iter()
            .map(|(name, size)| {
                serde_json::json!({
                    "name": name,
                    "size": size,
                    "browser_download_url": format!("{ASSET_URL_PREFIX}{tag}/{name}"),
                })
            })
            .collect();
        serde_json::to_vec(&serde_json::json!({
            "tag_name": tag,
            "html_url": format!("{RELEASES_URL}tag/{tag}"),
            "draft": false,
            "prerelease": false,
            "assets": assets,
            "body": "notes",
        }))
        .unwrap()
    }

    #[test]
    fn offers_only_a_newer_installable_release() {
        let current = v("0.1.0");
        let json = release(
            "v0.2.0",
            &[
                ("smolcast-0.2.0-x86_64.exe", 5_000_000),
                ("smolcast-0.2.0-x86_64.exe.sha256", 90),
            ],
        );
        let update = newer_release(&json, &current).unwrap().unwrap();
        assert_eq!(update.version, v("0.2.0"));
        assert_eq!(update.exe.size, 5_000_000);
        assert!(update.checksum.url.ends_with(".exe.sha256"));
        assert_eq!(update.page_url, format!("{RELEASES_URL}tag/v0.2.0"));

        // Assets or notes outside this repository's releases are not trusted.
        let text = String::from_utf8(json.clone()).unwrap();
        let elsewhere = text.replace(ASSET_URL_PREFIX, "https://example.com/");
        assert!(newer_release(elsewhere.as_bytes(), &current).is_err());
        let odd_page = text.replace(&format!("{RELEASES_URL}tag/"), "file:///C:/x/");
        let update = newer_release(odd_page.as_bytes(), &current)
            .unwrap()
            .unwrap();
        assert_eq!(update.page_url, RELEASES_URL);

        // Same or older: nothing to offer.
        assert_eq!(newer_release(&json, &v("0.2.0")).unwrap(), None);
        assert_eq!(newer_release(&json, &v("0.3.0")).unwrap(), None);

        // Newer, but missing the checksum or with a bad size: an error.
        let no_sum = release("v0.2.0", &[("smolcast-0.2.0-x86_64.exe", 5)]);
        assert!(newer_release(&no_sum, &current).is_err());
        let huge = release(
            "v0.2.0",
            &[
                ("smolcast-0.2.0-x86_64.exe", MAX_EXE_BYTES + 1),
                ("smolcast-0.2.0-x86_64.exe.sha256", 90),
            ],
        );
        assert!(newer_release(&huge, &current).is_err());
        assert!(newer_release(&release("latest", &[]), &current).is_err());
        assert!(newer_release(b"<html>", &current).is_err());
    }

    #[test]
    fn reads_sha256sum_files() {
        let hex = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        let digest = parse_checksum(&format!("{hex}  smolcast-0.2.0-x86_64.exe\n")).unwrap();
        assert_eq!(digest[0], 0x9f);
        assert_eq!(digest[31], 0x08);
        assert_eq!(parse_checksum(&hex.to_uppercase()), Some(digest));
        assert_eq!(parse_checksum(&hex[..63]), None);
        assert_eq!(parse_checksum(&format!("{}zz", &hex[..62])), None);
        assert_eq!(parse_checksum(""), None);
    }

    #[test]
    fn schedules_daily_checks_with_quicker_retries() {
        let hour = 60 * 60 * 1000;
        let started = 1_000 * hour;
        let fresh = UpdateState::default();
        // Never checked: right after the startup delay.
        assert_eq!(fresh.next_check_in(started, started), STARTUP_DELAY);
        let checked = UpdateState {
            last_check_ms: started - hour,
            ..fresh.clone()
        };
        assert_eq!(
            checked.next_check_in(started, started - 2 * hour),
            Duration::from_millis(23 * hour as u64)
        );
        let failed = UpdateState {
            last_check_failed: true,
            ..checked.clone()
        };
        assert_eq!(
            failed.next_check_in(started, started - 2 * hour),
            Duration::from_millis(hour as u64)
        );
        // Overdue: now, but still after the startup delay.
        let stale = UpdateState {
            last_check_ms: started - 48 * hour,
            ..fresh
        };
        assert_eq!(stale.next_check_in(started, started), STARTUP_DELAY);
    }

    #[test]
    fn skipped_versions_are_not_offered_and_state_round_trips() {
        let state = UpdateState {
            skipped: Some("0.2.0".into()),
            ..UpdateState::default()
        };
        assert!(!state.offers(&v("0.2.0")));
        assert!(state.offers(&v("0.2.1")));
        assert_eq!(UpdateState::decode(&state.encode()), state);
        assert_eq!(UpdateState::decode(b"junk"), UpdateState::default());
    }
}

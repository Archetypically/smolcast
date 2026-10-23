//! Crash reports (plan §14): the local panic log's format and the GitHub
//! issue Report a Bug prefills. Nothing here sends anything: the log stays
//! on disk, and an issue is filed only by the user, in the browser.

use std::fmt::Write as _;

use crate::core::web;

/// Past this size the log is set aside as the previous one, so at most
/// twice this is ever on disk.
pub const MAX_LOG_BYTES: u64 = 64 * 1024;
/// A longer panic message is cut: it can quote whatever failed to parse.
const MAX_MESSAGE_CHARS: usize = 2000;
/// The crash text an issue carries, counted percent-encoded. The whole URL
/// stays near 2 KB, which the shell passes to any browser intact.
const MAX_ISSUE_CRASH_ENCODED: usize = 1200;
/// Starts every entry's first line.
const ENTRY_MARK: &str = "=== smolcast crash, ";
/// Ends text that was cut short, in both the preview and the issue.
const CUT_MARK: &str = "… (cut)";

/// One panic, as the hook saw it.
pub struct Crash<'a> {
    pub unix_secs: u64,
    pub version: &'a str,
    pub windows: &'a str,
    pub thread: &'a str,
    pub location: &'a str,
    pub message: &'a str,
}

/// The newest entry of a log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastCrash {
    /// When it happened, as the entry's first line has it.
    pub when: String,
    /// The whole entry, first line included.
    pub text: String,
}

/// An entry for the log, ending in a blank line.
pub fn entry(crash: &Crash) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "{ENTRY_MARK}{}", utc_timestamp(crash.unix_secs));
    let _ = writeln!(text, "smolcast {} on {}", crash.version, crash.windows);
    let _ = writeln!(
        text,
        "thread '{}' panicked at {}:",
        crash.thread, crash.location
    );
    let _ = writeln!(text, "{}", cut(crash.message.trim_end(), MAX_MESSAGE_CHARS));
    text.push('\n');
    text
}

/// The last entry in `log`, if it has one.
pub fn last_entry(log: &str) -> Option<LastCrash> {
    let start = log
        .rmatch_indices(ENTRY_MARK)
        .map(|(i, _)| i)
        .find(|&i| i == 0 || log.as_bytes()[i - 1] == b'\n')?;
    let text = log[start..].trim_end();
    let first = text.lines().next().unwrap_or_default();
    Some(LastCrash {
        when: first[ENTRY_MARK.len()..].to_owned(),
        text: text.to_owned(),
    })
}

/// `YYYY-MM-DD HH:MM:SS UTC` for seconds since the Unix epoch.
pub fn utc_timestamp(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let secs = unix_secs % 86_400;
    // Days to a civil date (Howard Hinnant's `civil_from_days`).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

/// `Windows 11 24H2 (build 26100.2314)`. Windows 11 still names itself
/// "Windows 10" in the registry; build 22000 is the first Windows 11.
pub fn windows_label(build: u32, revision: Option<u32>, display_version: Option<&str>) -> String {
    let name = if build >= 22_000 {
        "Windows 11"
    } else {
        "Windows 10"
    };
    let mut label = name.to_owned();
    if let Some(version) = display_version.map(str::trim).filter(|v| !v.is_empty()) {
        let _ = write!(label, " {version}");
    }
    let _ = write!(label, " (build {build}");
    if let Some(revision) = revision {
        let _ = write!(label, ".{revision}");
    }
    label.push(')');
    label
}

/// The part of a crash entry an issue carries: exactly what Report a Bug
/// previews.
pub fn issue_excerpt(crash: &str) -> String {
    let crash = crash.trim_end();
    let mut encoded = 0;
    for (i, c) in crash.char_indices() {
        encoded += if c.is_ascii_alphanumeric() || "-_.~".contains(c) {
            1
        } else {
            3 * c.len_utf8()
        };
        if encoded > MAX_ISSUE_CRASH_ENCODED {
            return format!("{}{CUT_MARK}", &crash[..i]);
        }
    }
    crash.to_owned()
}

/// A new-issue URL on `repository` (`https://github.com/owner/name`), its
/// body an outline with the version, the Windows build and, when given,
/// a crash excerpt (`issue_excerpt`).
pub fn issue_url(repository: &str, version: &str, windows: &str, excerpt: Option<&str>) -> String {
    let mut body = String::from(
        "### What happened\n\n\n\
         ### What you expected\n\n\n\
         ### Steps to reproduce\n\n1. \n\n",
    );
    let _ = write!(body, "### Environment\n\nsmolcast {version}\n{windows}\n");
    if let Some(excerpt) = excerpt {
        // A longer fence than a panic message is likely to contain.
        let _ = write!(body, "\n### Crash report\n\n````text\n{excerpt}\n````\n");
    }
    format!(
        "{}/issues/new?body={}",
        repository.trim_end_matches('/'),
        web::encode(&body)
    )
}

/// `text` cut to `max` characters, marked when cut.
fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}{CUT_MARK}", &text[..end]),
        None => text.to_owned(),
    }
}

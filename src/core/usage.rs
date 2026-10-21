//! Launch-usage state schema v1: decode with duplicate merge, deterministic
//! encode, and saturating updates.
//!
//! Windows-free and host-testable. Usage is a cache, not user-authored config:
//! a malformed or unsupported file is quarantined by the persistence layer and
//! replaced with empty state (see `docs/systemPatterns.md` §10.4).
//!
//! Contract:
//! - `schema_version` must equal 1; unknown fields are ignored.
//! - Duplicate IDs on read merge with saturating count addition and the
//!   greatest timestamp.
//! - Entries are written sorted by `AppId`, so equal state encodes to equal bytes.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use serde::{Deserialize, Serialize};

use crate::core::app_index::AppId;

pub const USAGE_SCHEMA_VERSION: u32 = 1;

/// Launch statistics for one app.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UsageStats {
    pub launch_count: u64,
    /// UTC Unix milliseconds of the most recent successful launch.
    pub last_launch_unix_ms: i64,
}

#[derive(Debug)]
pub enum UsageError {
    /// Not JSON, or not the v1 document shape.
    Malformed(String),
    /// A well-formed document with a `schema_version` other than 1.
    UnsupportedVersion(u32),
}

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsageError::Malformed(message) => write!(f, "malformed usage state: {message}"),
            UsageError::UnsupportedVersion(v) => {
                write!(f, "unsupported usage schema_version {v}")
            }
        }
    }
}

impl std::error::Error for UsageError {}

/// In-memory usage state keyed by app identity.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Usage {
    entries: BTreeMap<AppId, UsageStats>,
}

#[derive(Deserialize)]
struct VersionProbe {
    schema_version: u32,
}

#[derive(Serialize, Deserialize)]
struct UsageFile {
    schema_version: u32,
    entries: Vec<UsageEntry>,
}

#[derive(Serialize, Deserialize)]
struct UsageEntry {
    id: AppId,
    launch_count: u64,
    last_launch_unix_ms: i64,
}

impl Usage {
    /// Decode a usage document. The version is checked before the entry shape
    /// so a future schema reports `UnsupportedVersion`, not `Malformed`.
    pub fn decode(bytes: &[u8]) -> Result<Self, UsageError> {
        let probe: VersionProbe =
            serde_json::from_slice(bytes).map_err(|e| UsageError::Malformed(e.to_string()))?;
        if probe.schema_version != USAGE_SCHEMA_VERSION {
            return Err(UsageError::UnsupportedVersion(probe.schema_version));
        }
        let file: UsageFile =
            serde_json::from_slice(bytes).map_err(|e| UsageError::Malformed(e.to_string()))?;

        let mut usage = Usage::default();
        for entry in file.entries {
            // Merge only true duplicates; a lone entry keeps its own values,
            // including a negative timestamp a default of 0 would clamp.
            match usage.entries.entry(entry.id) {
                Entry::Vacant(slot) => {
                    slot.insert(UsageStats {
                        launch_count: entry.launch_count,
                        last_launch_unix_ms: entry.last_launch_unix_ms,
                    });
                }
                Entry::Occupied(mut slot) => {
                    let stats = slot.get_mut();
                    stats.launch_count = stats.launch_count.saturating_add(entry.launch_count);
                    stats.last_launch_unix_ms =
                        stats.last_launch_unix_ms.max(entry.last_launch_unix_ms);
                }
            }
        }
        Ok(usage)
    }

    /// Encode deterministically: entries sorted by `AppId`, pretty-printed,
    /// trailing newline.
    pub fn encode(&self) -> Vec<u8> {
        let file = UsageFile {
            schema_version: USAGE_SCHEMA_VERSION,
            entries: self
                .entries
                .iter()
                .map(|(id, stats)| UsageEntry {
                    id: id.clone(),
                    launch_count: stats.launch_count,
                    last_launch_unix_ms: stats.last_launch_unix_ms,
                })
                .collect(),
        };
        let mut bytes =
            serde_json::to_vec_pretty(&file).expect("usage state always serializes to JSON");
        bytes.push(b'\n');
        bytes
    }

    pub fn get(&self, id: &AppId) -> Option<UsageStats> {
        self.entries.get(id).copied()
    }

    /// Record one successful launch at `now_ms` (§6.3): saturating count
    /// increment, and the timestamp becomes the launch time.
    pub fn record_launch(&mut self, id: &AppId, now_ms: i64) {
        let stats = self.entries.entry(id.clone()).or_default();
        stats.launch_count = stats.launch_count.saturating_add(1);
        stats.last_launch_unix_ms = now_ms;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packaged(key: &str) -> AppId {
        AppId::Packaged {
            aumid_key: key.into(),
        }
    }

    #[test]
    fn decode_merges_duplicates_and_ignores_unknown_fields() {
        let json = br#"{
            "schema_version": 1,
            "future_field": true,
            "entries": [
                {"id": {"kind": "packaged", "aumid_key": "a!app"}, "launch_count": 3, "last_launch_unix_ms": 100, "note": "x"},
                {"id": {"kind": "unresolved_shortcut", "path_key": "c:\\s\\x.lnk"}, "launch_count": 1, "last_launch_unix_ms": 5},
                {"id": {"kind": "packaged", "aumid_key": "a!app"}, "launch_count": 18446744073709551615, "last_launch_unix_ms": 50}
            ]
        }"#;
        let usage = Usage::decode(json).unwrap();
        assert_eq!(usage.len(), 2);
        assert_eq!(
            usage.get(&packaged("a!app")),
            Some(UsageStats {
                launch_count: u64::MAX,
                last_launch_unix_ms: 100,
            })
        );
    }

    #[test]
    fn decode_rejects_unsupported_version_and_malformed_input() {
        assert!(matches!(
            Usage::decode(br#"{"schema_version": 2, "entries": "new shape"}"#),
            Err(UsageError::UnsupportedVersion(2))
        ));
        assert!(matches!(
            Usage::decode(b"not json"),
            Err(UsageError::Malformed(_))
        ));
        assert!(matches!(
            Usage::decode(br#"{"schema_version": 1, "entries": [{"id": 7}]}"#),
            Err(UsageError::Malformed(_))
        ));
    }

    #[test]
    fn decode_keeps_a_lone_negative_timestamp() {
        let mut usage = Usage::default();
        let app = AppId::UnresolvedShortcut {
            path_key: "a".into(),
        };
        usage.record_launch(&app, -5);
        assert_eq!(Usage::decode(&usage.encode()).unwrap(), usage);
    }

    #[test]
    fn encode_is_sorted_deterministic_and_round_trips() {
        let classic = AppId::Classic {
            target_key: "c:\\t.exe".into(),
            arguments: "-x".into(),
            cwd_key: "c:\\".into(),
        };
        let mut first = Usage::default();
        first.record_launch(&packaged("z!app"), 10);
        first.record_launch(&classic, 20);
        first.record_launch(&packaged("a!app"), 30);

        let mut second = Usage::default();
        second.record_launch(&packaged("a!app"), 30);
        second.record_launch(&packaged("z!app"), 10);
        second.record_launch(&classic, 20);

        let bytes = first.encode();
        assert_eq!(bytes, second.encode());
        assert!(bytes.ends_with(b"}\n"));
        assert_eq!(Usage::decode(&bytes).unwrap(), first);

        // Sorted by derived AppId order: Packaged variants precede Classic.
        let text = String::from_utf8(bytes).unwrap();
        let a = text.find("a!app").unwrap();
        let z = text.find("z!app").unwrap();
        let t = text.find("t.exe").unwrap();
        assert!(a < z && z < t);
    }

    #[test]
    fn record_launch_saturates_and_sets_latest_time() {
        let id = packaged("a!app");
        let mut usage = Usage::decode(
            br#"{"schema_version": 1, "entries": [{"id": {"kind": "packaged", "aumid_key": "a!app"}, "launch_count": 18446744073709551615, "last_launch_unix_ms": 9000}]}"#,
        )
        .unwrap();
        usage.record_launch(&id, 1000);
        assert_eq!(
            usage.get(&id),
            Some(UsageStats {
                launch_count: u64::MAX,
                last_launch_unix_ms: 1000,
            })
        );
    }
}

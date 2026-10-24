//! Hidden entries: palette entries the user took out of the root list, in
//! the order they were hidden.
//!
//! Windows-free and host-testable. Like favorites (decision 011) they are
//! user-curated, so they live next to config.toml (`%APPDATA%\smolcast\
//! hidden.json`). Each entry keeps the name it had when hidden, so the
//! Hidden Entries view can list it while its app is not installed.

use serde::{Deserialize, Serialize};

use crate::core::app_index::AppId;

pub const HIDDEN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, PartialEq, Eq)]
pub enum HiddenError {
    Malformed(String),
    UnsupportedVersion(u32),
}

impl std::fmt::Display for HiddenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(m) => write!(f, "malformed hidden entries: {m}"),
            Self::UnsupportedVersion(v) => {
                write!(f, "unsupported hidden entries schema_version {v}")
            }
        }
    }
}

impl std::error::Error for HiddenError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiddenEntry {
    pub id: AppId,
    /// The display name when it was hidden.
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hidden {
    entries: Vec<HiddenEntry>,
}

#[derive(Deserialize)]
struct VersionProbe {
    schema_version: u32,
}

#[derive(Serialize, Deserialize)]
struct HiddenFile {
    schema_version: u32,
    hidden: Vec<HiddenEntry>,
}

impl Hidden {
    /// Decode a hidden-entries document. The version is checked before the
    /// entry shape so a future schema reports `UnsupportedVersion`.
    pub fn decode(bytes: &[u8]) -> Result<Self, HiddenError> {
        let probe: VersionProbe =
            serde_json::from_slice(bytes).map_err(|e| HiddenError::Malformed(e.to_string()))?;
        if probe.schema_version != HIDDEN_SCHEMA_VERSION {
            return Err(HiddenError::UnsupportedVersion(probe.schema_version));
        }
        let file: HiddenFile =
            serde_json::from_slice(bytes).map_err(|e| HiddenError::Malformed(e.to_string()))?;
        let mut hidden = Self::default();
        for entry in file.hidden {
            hidden.hide(entry.id, entry.name);
        }
        Ok(hidden)
    }

    pub fn encode(&self) -> Vec<u8> {
        let file = HiddenFile {
            schema_version: HIDDEN_SCHEMA_VERSION,
            hidden: self.entries.clone(),
        };
        serde_json::to_vec_pretty(&file).expect("hidden entries serialize")
    }

    /// Every hidden entry in the order hidden, including ones not installed now.
    pub fn entries(&self) -> &[HiddenEntry] {
        &self.entries
    }

    pub fn contains(&self, id: &AppId) -> bool {
        self.entries.iter().any(|e| &e.id == id)
    }

    pub fn hide(&mut self, id: AppId, name: String) {
        if !self.contains(&id) {
            self.entries.push(HiddenEntry { id, name });
        }
    }

    pub fn unhide(&mut self, id: &AppId) {
        self.entries.retain(|e| &e.id != id);
    }
}

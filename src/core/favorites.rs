//! Favorites: palette entries pinned to the top of the root list, in the
//! user's order.
//!
//! Windows-free and host-testable. Favorites are user-curated, so unlike the
//! usage cache they live next to config.toml (`%APPDATA%\smolcast\
//! favorites.json`). An entry whose app is not installed right now stays
//! saved but is neither shown nor counted, so reinstalling brings it back.

use serde::{Deserialize, Serialize};

use crate::core::app_index::AppId;

pub const FAVORITES_SCHEMA_VERSION: u32 = 1;
/// Shown favorites at most: enough for daily apps without crowding out
/// suggestions.
pub const MAX_FAVORITES: usize = 5;

#[derive(Debug, PartialEq, Eq)]
pub enum FavoritesError {
    Malformed(String),
    UnsupportedVersion(u32),
}

impl std::fmt::Display for FavoritesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(m) => write!(f, "malformed favorites: {m}"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported favorites schema_version {v}"),
        }
    }
}

impl std::error::Error for FavoritesError {}

/// Adding failed: this many favorites are already shown.
#[derive(Debug, PartialEq, Eq)]
pub struct Full;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Favorites {
    ids: Vec<AppId>,
}

#[derive(Serialize, Deserialize)]
struct FavoritesFile {
    schema_version: u32,
    favorites: Vec<AppId>,
}

impl Favorites {
    pub fn decode(bytes: &[u8]) -> Result<Self, FavoritesError> {
        let file: FavoritesFile =
            serde_json::from_slice(bytes).map_err(|e| FavoritesError::Malformed(e.to_string()))?;
        if file.schema_version != FAVORITES_SCHEMA_VERSION {
            return Err(FavoritesError::UnsupportedVersion(file.schema_version));
        }
        let mut favorites = Self::default();
        for id in file.favorites {
            if !favorites.ids.contains(&id) {
                favorites.ids.push(id);
            }
        }
        Ok(favorites)
    }

    pub fn encode(&self) -> Vec<u8> {
        let file = FavoritesFile {
            schema_version: FAVORITES_SCHEMA_VERSION,
            favorites: self.ids.clone(),
        };
        serde_json::to_vec_pretty(&file).expect("favorites serialize")
    }

    /// Every saved favorite in order, including ones not installed now.
    pub fn ids(&self) -> &[AppId] {
        &self.ids
    }

    pub fn contains(&self, id: &AppId) -> bool {
        self.ids.contains(id)
    }

    /// Pin `id` at the end, unless `shown` favorites (those installed now)
    /// already reach the limit.
    pub fn add(&mut self, id: AppId, shown: usize) -> Result<(), Full> {
        if self.contains(&id) {
            return Ok(());
        }
        if shown >= MAX_FAVORITES {
            return Err(Full);
        }
        self.ids.push(id);
        Ok(())
    }

    pub fn remove(&mut self, id: &AppId) {
        self.ids.retain(|x| x != id);
    }

    /// Swap `id` with the nearest favorite in `direction` (-1 up, +1 down)
    /// that `is_shown` accepts, so hidden entries never absorb a move.
    /// Returns whether anything moved.
    pub fn shift(&mut self, id: &AppId, up: bool, is_shown: impl Fn(&AppId) -> bool) -> bool {
        let Some(from) = self.ids.iter().position(|x| x == id) else {
            return false;
        };
        let target = if up {
            (0..from).rev().find(|&i| is_shown(&self.ids[i]))
        } else {
            (from + 1..self.ids.len()).find(|&i| is_shown(&self.ids[i]))
        };
        match target {
            Some(to) => {
                self.ids.swap(from, to);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> AppId {
        AppId::UnresolvedShortcut {
            path_key: name.into(),
        }
    }

    #[test]
    fn round_trips_in_order_and_drops_duplicates() {
        let mut favorites = Favorites::default();
        for name in ["b", "a", "c"] {
            favorites.add(id(name), favorites.ids().len()).unwrap();
        }
        let decoded = Favorites::decode(&favorites.encode()).unwrap();
        assert_eq!(decoded, favorites);

        let text = br#"{"schema_version":1,"favorites":[
            {"kind":"unresolved_shortcut","path_key":"a"},
            {"kind":"unresolved_shortcut","path_key":"a"}]}"#;
        assert_eq!(Favorites::decode(text).unwrap().ids(), &[id("a")]);
    }

    #[test]
    fn rejects_bad_documents() {
        assert!(matches!(
            Favorites::decode(b"nope"),
            Err(FavoritesError::Malformed(_))
        ));
        assert_eq!(
            Favorites::decode(br#"{"schema_version":2,"favorites":[]}"#),
            Err(FavoritesError::UnsupportedVersion(2))
        );
    }

    #[test]
    fn the_limit_counts_shown_favorites_only() {
        let mut favorites = Favorites::default();
        for i in 0..MAX_FAVORITES {
            favorites.add(id(&i.to_string()), i).unwrap();
        }
        assert_eq!(favorites.add(id("x"), MAX_FAVORITES), Err(Full));
        // One favorite is uninstalled right now: there is room again.
        assert_eq!(favorites.add(id("x"), MAX_FAVORITES - 1), Ok(()));
        // Re-adding an existing favorite is a no-op, never Full.
        assert_eq!(favorites.add(id("0"), MAX_FAVORITES), Ok(()));
    }

    #[test]
    fn shifting_skips_hidden_entries() {
        let mut favorites = Favorites::default();
        for name in ["a", "hidden", "b"] {
            favorites.add(id(name), 0).unwrap();
        }
        let shown = |x: &AppId| *x != id("hidden");
        assert!(favorites.shift(&id("b"), true, shown));
        assert_eq!(favorites.ids(), &[id("b"), id("hidden"), id("a")]);
        assert!(!favorites.shift(&id("b"), true, shown));
        favorites.remove(&id("hidden"));
        assert_eq!(favorites.ids(), &[id("b"), id("a")]);
    }
}

//! The model's outbox (plan §1.2): work the host executes for the model.
//!
//! The model never starts threads or touches Windows resources itself; it
//! queues an `Effect` and the host drains them all, in order, once per step
//! with `AppModel::take_effects`. Outcomes come back as `AppEvent`s or
//! through the model's `*_failed` methods.

use std::path::PathBuf;

use crate::core::config::ConfigFile;
use crate::core::config_edit::ConfigEdit;
use crate::win::clipboard::ClipboardCommand;
use crate::win::persistence::ClipboardStoreCommand;

use super::clipboard_view::ThumbnailRequest;
use super::emoji_view::GlyphRequest;
use super::launcher::{IconRequest, LaunchRequest};
use super::model::{PasteFinisherRequest, UpdateRequest};

pub enum Effect {
    /// Re-scan the Start Menu for this generation (§6.4).
    Scan(u64),
    /// Move broken shortcuts to the Recycle Bin.
    Recycle(Vec<PathBuf>),
    Launch(LaunchRequest),
    Icon(IconRequest),
    /// An Explorer icon for a Search Files row: path, pixel edge.
    FileIcon(String, u32),
    Glyph(GlyphRequest),
    /// Encoded usage to write, with its generation (§10.4).
    SaveUsage(u64, Vec<u8>),
    SaveFavorites(Vec<u8>),
    /// A file query for the Windows Search worker: generation, text (§6.6).
    FileSearch(u64, String),
    Update(UpdateRequest),
    Clipboard(ClipboardCommand),
    Store(ClipboardStoreCommand),
    Thumbnail(ThumbnailRequest),
    /// Inject the paste once the clipboard write landed (§8.2 steps 6–8).
    /// Pushed while reducing the write receipt, so it starts in that step.
    PasteFinisher(PasteFinisherRequest),
    /// A form edit to write to config.toml; a reload follows (§10.6).
    SaveConfig(ConfigEdit),
    /// Re-read config.toml off the UI thread (§5.2).
    RefreshConfig,
    OpenConfigFile,
    /// Show the Open dialog for a script file to track (§10.3).
    PickScriptFile,
    /// A re-read config to apply: hotkeys re-register first (§5.2).
    ApplyConfig(ConfigFile),
    /// A session-only summon key from the conflict UI.
    TrialHotkey(String),
}

//! Host-testable core data logic. All modules here are Windows-free.

pub mod app_index;
pub mod calc;
pub mod config;
pub mod config_edit;
pub mod dib;
pub mod emoji;
pub mod favorites;
pub mod file_search;
#[rustfmt::skip]
pub mod emoji_data;
pub mod history;
pub mod icon_scale;
pub mod installed_programs;
pub mod list_cursor;
pub mod matcher;
pub mod motion;
pub mod placement;
pub mod quicklink;
pub mod request_cache;
pub mod script_file;
pub mod snippet;
pub mod update;
pub mod usage;
pub mod web;

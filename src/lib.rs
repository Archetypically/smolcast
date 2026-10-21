//! smolcast — minimal Windows-native command palette.
//!
//! The library target exposes the Windows-free core data logic so it can be
//! unit-tested on the host (Linux). The shipped binary is Windows-only and lives
//! in `src/`, importing this library as `smolcast::core`.
//!
//! Invariant (see `docs/systemPatterns.md` §4): `lib.rs` and every `core`
//! module compile without `cfg(windows)` and without importing the `windows`,
//! eframe, egui, tray, or hotkey crates.

pub mod core;

// The app shell and the Windows API layer are part of the shipped binary and
// are only compiled into Windows targets; on the Linux host `cargo test --lib`
// exercises the core modules alone.
#[cfg(windows)]
pub mod app;

#[cfg(windows)]
pub mod win;

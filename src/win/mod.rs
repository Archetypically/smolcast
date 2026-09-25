//! Windows API layer. Every `unsafe` call lives under this tree; safe modules
//! receive owned values and return typed errors. Handles, registered hotkeys,
//! clipboard ownership, COM, window classes, timers, and hooks use RAII guards
//! where ownership applies.

pub mod appearance;
pub mod autostart;
pub mod clipboard;
pub mod emoji_render;
pub mod hotkey;
pub mod icon;
pub mod paste;
pub mod persistence;
pub mod placement;
pub mod process;
pub mod shell;
pub mod single_instance;
pub mod tray;

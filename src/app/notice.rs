//! Bounded, content-safe user notices (plan §5.3).
//!
//! Notices never carry clipboard text/image bytes or full script arguments.
//! Diagnostics may include an operation code, a numeric error code, sizes, ids,
//! and counts.

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A user-facing notice. `message` is content-safe by contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub severity: Severity,
    /// Stable operation code, e.g. `op::SUMMON_HOTKEY`.
    pub operation: &'static str,
    pub message: String,
    /// Optional Win32 error or HRESULT numeric code.
    pub code: Option<u32>,
}

impl Notice {
    pub fn new(severity: Severity, operation: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity,
            operation,
            message: message.into(),
            code: None,
        }
    }

    pub fn with_code(mut self, code: u32) -> Self {
        self.code = Some(code);
        self
    }

    pub fn with_optional_code(mut self, code: Option<u32>) -> Self {
        self.code = code;
        self
    }
}

/// Stable operation codes for diagnostics and UI grouping.
pub mod op {
    pub const STARTUP: &str = "startup";
    pub const SUMMON_HOTKEY: &str = "summon_hotkey";
    pub const CONFIG: &str = "config";
    pub const TRAY: &str = "tray";
    pub const CLIPBOARD: &str = "clipboard";
    pub const LAUNCHER: &str = "launcher";
    pub const USAGE: &str = "usage";
    pub const SCRIPTS: &str = "scripts";
}

/// Maximum number of notices retained in memory (§5.3).
pub const MAX_NOTICES: usize = 32;

#[derive(Debug, Default)]
pub struct NoticeStore {
    queue: VecDeque<Notice>,
}

impl NoticeStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert, dropping the oldest notice when at capacity.
    pub fn push(&mut self, notice: Notice) {
        if self.queue.len() == MAX_NOTICES {
            self.queue.pop_front();
        }
        self.queue.push_back(notice);
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Notice> {
        self.queue.iter()
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_bounds_to_max_notices() {
        let mut store = NoticeStore::new();
        for i in 0..(MAX_NOTICES + 5) {
            store.push(Notice::new(
                Severity::Info,
                op::STARTUP,
                format!("notice {i}"),
            ));
        }
        assert_eq!(store.len(), MAX_NOTICES);
        // Oldest dropped first: the first surviving notice is index 5.
        assert!(store.iter().next().unwrap().message.ends_with('5'));
    }
}

//! Global hotkeys: main-thread manager, id→action route table, and handler
//! wakeups (plan §1.1, §1.2, §5.2) for the summon key and optional per-script
//! keys (§10.3).

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

use global_hotkey::hotkey::{HotKey, HotKeyParseError};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use parking_lot::Mutex;

use crate::app::model::{AppEvent, Wake};

/// What a registered global hotkey id triggers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyAction {
    Summon,
    /// Run the script with this config id.
    Script(String),
}

/// id → action map shared with the global event handler. The handler looks the
/// id up, maps it to a typed `AppEvent`, and enqueues it with a repaint request
/// (§1.2 producer contract).
#[derive(Clone, Debug)]
pub struct Routes(Arc<Mutex<BTreeMap<u32, HotkeyAction>>>);

impl Routes {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(BTreeMap::new())))
    }

    fn lookup(&self, id: u32) -> Option<HotkeyAction> {
        self.0.lock().get(&id).cloned()
    }

    fn insert(&self, id: u32, action: HotkeyAction) {
        self.0.lock().insert(id, action);
    }

    fn remove(&self, id: u32) {
        self.0.lock().remove(&id);
    }

    fn clear(&self) {
        self.0.lock().clear();
    }
}

#[derive(Debug)]
pub enum HotkeyError {
    Parse(HotKeyParseError),
    /// `GlobalHotKeyManager` creation or a registration conflict.
    Register(String),
    Unregister(String),
}

impl std::fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HotkeyError::Parse(e) => write!(f, "invalid hotkey string: {e}"),
            HotkeyError::Register(m) => write!(f, "hotkey registration failed: {m}"),
            HotkeyError::Unregister(m) => write!(f, "hotkey unregistration failed: {m}"),
        }
    }
}

impl std::error::Error for HotkeyError {}

/// Install the global hotkey event handler. Must run on the main thread after
/// the manager exists. On shutdown, call [`clear_event_handler`] before dropping
/// senders or the manager (§1.5 step 2).
pub fn install_event_handler(wake: Wake, routes: Routes) {
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state == HotKeyState::Pressed
            && let Some(action) = routes.lookup(event.id())
        {
            let app_event = match action {
                HotkeyAction::Summon => AppEvent::SummonRequested,
                HotkeyAction::Script(id) => AppEvent::RunScript(id),
            };
            wake.send(app_event);
        }
    }));
}

/// Remove the global handler (shutdown only).
pub fn clear_event_handler() {
    GlobalHotKeyEvent::set_event_handler(None::<fn(GlobalHotKeyEvent)>);
}

/// Owns the `GlobalHotKeyManager` and current summon key. Lives on the main
/// thread for the life of the app (§1.1).
pub struct HotkeyManager {
    manager: GlobalHotKeyManager,
    routes: Routes,
    summon: Option<HotKey>,
    /// Registered script keys: config id → (key string, hotkey).
    scripts: BTreeMap<String, (String, HotKey)>,
}

/// A script hotkey that could not be registered; only that script's hotkey is
/// disabled (§10.3).
#[derive(Debug)]
pub struct ScriptHotkeyFailure {
    pub script_id: String,
    pub key: String,
    pub error: HotkeyError,
}

impl HotkeyManager {
    pub fn new() -> Result<Self, HotkeyError> {
        let manager =
            GlobalHotKeyManager::new().map_err(|e| HotkeyError::Register(e.to_string()))?;
        Ok(Self {
            manager,
            routes: Routes::new(),
            summon: None,
            scripts: BTreeMap::new(),
        })
    }

    /// Route table for the caller installing the global handler.
    pub fn routes(&self) -> Routes {
        self.routes.clone()
    }

    /// Current summon key id, if registered.
    pub fn summon_hotkey_id(&self) -> Option<u32> {
        self.summon.as_ref().map(|hk| hk.id())
    }

    /// Replace the summon key (§5.2): parse and register the new key *before*
    /// unregistering the old one, so a parse error or registration conflict
    /// leaves the previous registration fully intact.
    pub fn set_summon(&mut self, key: &str) -> Result<HotKey, HotkeyError> {
        let hk = HotKey::from_str(key).map_err(HotkeyError::Parse)?;
        self.manager
            .register(hk)
            .map_err(|e| HotkeyError::Register(e.to_string()))?;
        if let Some(old) = self.summon.take() {
            let _ = self.manager.unregister(old);
            if old.id() != hk.id() {
                self.routes.remove(old.id());
            }
        }
        self.routes.insert(hk.id(), HotkeyAction::Summon);
        self.summon = Some(hk);
        Ok(hk)
    }

    /// Bring script registrations in line with `wanted` (config id → key
    /// string). Unchanged keys stay registered; removed or changed ones are
    /// unregistered first; new ones register individually. The summon key is
    /// never touched.
    pub fn set_scripts(&mut self, wanted: &BTreeMap<String, String>) -> Vec<ScriptHotkeyFailure> {
        let stale: Vec<String> = self
            .scripts
            .iter()
            .filter(|(id, (key, _))| wanted.get(*id) != Some(key))
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale {
            if let Some((_, hk)) = self.scripts.remove(&id) {
                let _ = self.manager.unregister(hk);
                self.routes.remove(hk.id());
            }
        }
        let mut failures = Vec::new();
        for (id, key) in wanted {
            if self.scripts.contains_key(id) {
                continue;
            }
            let registered = HotKey::from_str(key)
                .map_err(HotkeyError::Parse)
                .and_then(|hk| {
                    self.manager
                        .register(hk)
                        .map(|()| hk)
                        .map_err(|e| HotkeyError::Register(e.to_string()))
                });
            match registered {
                Ok(hk) => {
                    self.routes
                        .insert(hk.id(), HotkeyAction::Script(id.clone()));
                    self.scripts.insert(id.clone(), (key.clone(), hk));
                }
                Err(error) => failures.push(ScriptHotkeyFailure {
                    script_id: id.clone(),
                    key: key.clone(),
                    error,
                }),
            }
        }
        failures
    }

    /// Unregister script keys, then the summon key, and clear the route table
    /// (shutdown, §1.5 step 3).
    pub fn unregister_all(&mut self) {
        for (_, (_, hk)) in std::mem::take(&mut self.scripts) {
            let _ = self.manager.unregister(hk);
        }
        if let Some(hk) = self.summon.take() {
            let _ = self.manager.unregister(hk);
        }
        self.routes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_and_invalid_keys() {
        assert!(HotKey::from_str("control+alt+Space").is_ok());
        assert!(HotKey::from_str("shift+alt+KeyQ").is_ok());
        assert!(HotKey::from_str("Shift+Ctrl").is_err());
        assert!(HotKey::from_str("not a key").is_err());
    }

    #[test]
    fn routes_replace_one_action() {
        let routes = Routes::new();
        routes.insert(7, HotkeyAction::Summon);
        assert_eq!(routes.lookup(7), Some(HotkeyAction::Summon));
        routes.remove(7);
        assert_eq!(routes.lookup(7), None);
    }
}

//! Tray icon, menu, and their handler wakeups (plan §5.2). Created and owned on
//! the main thread for the life of the app. Left-button release on the icon also
//! opens the palette; the menu offers Open palette, Refresh, and Quit.

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::app::model::{AppEvent, TrayCommand, Wake};

pub const MENU_OPEN: &str = "smolcast.open";
pub const MENU_REFRESH: &str = "smolcast.refresh";
pub const MENU_QUIT: &str = "smolcast.quit";

#[derive(Debug)]
pub struct TrayError {
    message: String,
}

impl std::fmt::Display for TrayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TrayError {}

/// Installs both tray event handlers, routing to typed events with a repaint
/// request each (§1.2). Calls once, on the main thread at startup. Tray
/// hover/move events are ignored; only left-button release and menu selections
/// are enqueued.
pub fn install_event_handlers(wake: Wake) {
    let menu_wake = wake.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let command = match event.id().0.as_str() {
            MENU_OPEN => TrayCommand::OpenPalette,
            MENU_REFRESH => TrayCommand::Refresh,
            MENU_QUIT => TrayCommand::Quit,
            _ => return,
        };
        menu_wake.send(AppEvent::Tray(command));
    }));

    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button,
            button_state,
            ..
        } = event
            && button == MouseButton::Left
            && button_state == MouseButtonState::Up
        {
            wake.send(AppEvent::Tray(TrayCommand::OpenPalette));
        }
    }));
}

/// Remove both tray handlers (shutdown).
pub fn clear_event_handlers() {
    MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
    TrayIconEvent::set_event_handler(None::<fn(TrayIconEvent)>);
}

/// Owns the tray icon; dropping it removes the icon from the notification area.
pub struct TrayHandle {
    _inner: tray_icon::TrayIcon,
}

impl TrayHandle {
    pub fn build() -> Result<Self, TrayError> {
        let menu = Menu::new();
        let open = MenuItem::with_id(MENU_OPEN, "Open palette", true, None);
        let refresh = MenuItem::with_id(MENU_REFRESH, "Refresh", true, None);
        let quit = MenuItem::with_id(MENU_QUIT, "Quit", true, None);
        menu.append_items(&[&open, &refresh, &quit])
            .map_err(|e| TrayError {
                message: e.to_string(),
            })?;

        let inner = TrayIconBuilder::new()
            .with_id("smolcast-main")
            .with_tooltip("smolcast")
            .with_icon(build_icon().map_err(TrayError::from_bad_icon)?)
            .with_menu(Box::new(menu))
            // Left click must not open the menu; it is handled as a palette
            // summon by the click handler above (§5.2).
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| TrayError {
                message: e.to_string(),
            })?;

        Ok(Self { _inner: inner })
    }
}

impl TrayError {
    fn from_bad_icon(_: tray_icon::BadIcon) -> Self {
        TrayError {
            message: "could not build tray icon bitmap".into(),
        }
    }
}

/// A minimal placeholder tray icon: dark rounded square with a lighter inner
/// grid, 32x32 RGBA. Replaced by a proper asset in the polish milestone.
fn build_icon() -> Result<Icon, tray_icon::BadIcon> {
    const SIZE: u32 = 32;
    const RADIUS: u32 = 7;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let px = (y * SIZE + x) as usize * 4;
            let corner_dist = corner_distance(x, y, SIZE, RADIUS);
            if corner_dist > RADIUS * RADIUS {
                rgba[px..px + 4].copy_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            // Background: dark translucent blue-gray.
            let (r, g, b, a) = (24, 28, 36, 255);
            // Inner 4x4 launcher dots in a lighter tone.
            let grid = (8..14).contains(&x) as u8 + (18..24).contains(&x) as u8;
            let grow = (8..14).contains(&y) as u8 + (18..24).contains(&y) as u8;
            if grid > 0 && grow > 0 {
                rgba[px..px + 4].copy_from_slice(&[120, 200, 255, 255]);
            } else {
                rgba[px..px + 4].copy_from_slice(&[r, g, b, a]);
            }
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE)
}

/// Squared distance to the nearest rounded-rectangle corner; inside corners
/// score 0.
fn corner_distance(x: u32, y: u32, size: u32, radius: u32) -> u32 {
    let cx = x.min(size - 1 - x);
    let cy = y.min(size - 1 - y);
    if cx >= radius && cy >= radius {
        0
    } else {
        let dx = radius.saturating_sub(cx) as i64;
        let dy = radius.saturating_sub(cy) as i64;
        (dx * dx + dy * dy) as u32
    }
}

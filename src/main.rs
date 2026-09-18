//! smolcast entry point (plan §5): single-instance guard, config bootstrap, the
//! shared egui context, clipboard worker startup with ready handshake, and
//! `eframe::run_native_ext`.
//!
//! This file is Windows-only; the host builds it through the GNU or MSVC cross
//! targets. `main` imports library code as `smolcast::core` and must not declare
//! a second `mod core`.

// GUI subsystem: no console window appears, so a second launch is fully silent
// and the resident app never flashes a terminal. Fatal bootstrap failures are
// reported via a message box because no eframe window exists yet.
#![windows_subsystem = "windows"]

use std::sync::mpsc;

use eframe::egui;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
use windows::core::PCWSTR;

use smolcast::app::eframe_host::EframeApp;
use smolcast::app::model::{AppEvent, AppModel};
use smolcast::app::notice::{Notice, Severity, op};
use smolcast::core::config::{self, ConfigFile};
use smolcast::win::clipboard::ClipboardThread;
use smolcast::win::single_instance::{AcquireError, SingleInstance};

/// Show a fatal startup error and exit. `stderr` alone cannot reach a user in a
/// GUI-subsystem process.
fn fatal(message: &str) -> ! {
    let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let caption: Vec<u16> = "smolcast — startup failed"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
    std::process::exit(1);
}

fn main() -> eframe::Result {
    // 1. Single instance, acquired before any UI or resource (§5.1). A second
    //    launch exits code 0 silently; the guard is held until run_native returns.
    let _instance = match SingleInstance::acquire() {
        Ok(instance) => instance,
        Err(AcquireError::AlreadyRunning) => std::process::exit(0),
        Err(e) => fatal(&format!("{e}")),
    };

    // 2. Config path (§10.1): %APPDATA%\smolcast\config.toml.
    let base_dirs = directories::BaseDirs::new().expect("no base directories");
    let config_path = base_dirs.config_dir().join("smolcast").join("config.toml");

    // 3. Config bootstrap (§2.1, §10.2): a malformed file yields safe defaults,
    //    no scripts, a visible error notice, and a forced palette.
    let (config, startup_notice, force_visible) = match config::load(&config_path) {
        Ok(cfg) => (cfg, None, false),
        Err(e) => (
            ConfigFile::default(),
            Some(Notice::new(
                Severity::Error,
                op::CONFIG,
                format!("Config error: {e}"),
            )),
            true,
        ),
    };

    // 4. Shared egui context and event channel (§1.2). Producers clone the
    //    sender and the context; the same context is handed to eframe below.
    let (event_tx, event_rx) = mpsc::channel::<AppEvent>();
    let ctx = egui::Context::default();

    // 5. Clipboard worker with ready handshake (§1.3). Milestone 1 requires its
    //    class registration and HWND to succeed, so failure aborts startup.
    let clipboard = match ClipboardThread::start() {
        Ok(worker) => worker,
        Err(e) => fatal(&format!("Clipboard worker failed to start: {e}")),
    };

    // 6. Root viewport: hidden, undecorated, always-on-top palette (§2.1). The
    //    first summon makes it visible; `X` hides; tray Quit closes.
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("smolcast")
            .with_app_id("smolcast")
            .with_inner_size([440.0, 200.0])
            .with_visible(false)
            .with_decorations(false)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false),
        ..Default::default()
    };

    match eframe::run_native_ext(
        "smolcast",
        options,
        Some(ctx),
        Box::new(move |cc| {
            let model = AppModel::new(event_rx, config, startup_notice, force_visible);
            let app = EframeApp::build(cc, model, event_tx, config_path, clipboard);
            Ok(Box::new(app))
        }),
    ) {
        Ok(()) => Ok(()),
        Err(e) => fatal(&format!("The main window failed to start: {e}")),
    }
}

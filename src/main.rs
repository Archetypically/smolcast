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

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use eframe::egui;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
use windows::core::PCWSTR;

use smolcast::app::eframe_host::{EframeApp, Startup};
use smolcast::app::model::{AppEvent, AppModel, Wake};
use smolcast::app::notice::{Notice, Severity, op};
use smolcast::app::report_view::ReportView;
use smolcast::app::theme;
use smolcast::core::config::{self, ConfigFile};
use smolcast::win::clipboard::ClipboardThread;
use smolcast::win::crash_log;
use smolcast::win::persistence::{self, StatePaths};
use smolcast::win::single_instance::{AcquireError, SingleInstance};
use smolcast::win::update;
use smolcast::win::wide;
use smolcast::win::{appearance, autostart};

/// Show a fatal startup error and exit. `stderr` alone cannot reach a user in a
/// GUI-subsystem process.
fn fatal(message: &str) -> ! {
    let text = wide(message);
    let caption = wide("smolcast — startup failed");
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
    //    A build started by the updater first waits for the old one to exit (§13).
    update::wait_for_previous_instance();
    let _instance = match SingleInstance::acquire() {
        Ok(instance) => instance,
        Err(AcquireError::AlreadyRunning) => std::process::exit(0),
        Err(e) => fatal(&format!("{e}")),
    };

    // 2. State paths (§10.1): %APPDATA%\smolcast\config.toml and
    //    %LOCALAPPDATA%\smolcast\usage.json.
    let Some(paths) = StatePaths::resolve() else {
        fatal("Could not locate the AppData folders.");
    };

    // Panics from here on are logged locally before the abort (§14).
    let windows = crash_log::windows_label();
    crash_log::install(paths.crash_log.clone(), windows.clone());
    let report = ReportView::new(windows, crash_log::last_crash(&paths.crash_log));

    // 3. Config bootstrap (§2.1, §10.2): a malformed file yields safe defaults,
    //    no scripts, a visible error notice, and a forced palette.
    let mut startup_notices = Vec::new();
    let (config, config_failed) = match config::load(&paths.config) {
        Ok(cfg) => (cfg, false),
        Err(e) => {
            startup_notices.push(Notice::new(
                Severity::Error,
                op::CONFIG,
                format!("Config error: {e}"),
            ));
            (ConfigFile::default(), true)
        }
    };

    // Usage is a cache (§10.4): a bad file is quarantined and reset with one
    // notice, which is shown on the next summon rather than forcing the window.
    let (usage, usage_notice) = persistence::load_usage(&paths.usage);
    startup_notices.extend(usage_notice);
    let (favorites, favorites_notice) = persistence::load_favorites(&paths.favorites);
    startup_notices.extend(favorites_notice);
    if crash_log::take_unseen(&paths.crash_log) {
        startup_notices.push(Notice::new(
            Severity::Warning,
            op::STARTUP,
            "smolcast quit unexpectedly last time. Report a Bug can include what went wrong.",
        ));
    }

    // A start by the user opens the palette, so they see smolcast started;
    // a start at sign-in (the Run entry's flag) or by the updater
    // stays in the tray. A config error forces it either way (decision 037).
    let quiet_start = std::env::args()
        .skip(1)
        .any(|arg| arg == autostart::LOGIN_FLAG || arg == update::WAIT_PID_FLAG);
    let force_visible = config_failed || !quiet_start;

    // 4. Shared egui context and event channel (§1.2). Producers clone the
    //    sender and the context; the same context is handed to eframe below.
    let (event_tx, event_rx) = mpsc::channel::<AppEvent>();
    let ctx = egui::Context::default();

    // 5. Clipboard worker with ready handshake (§1.3): class, HWND, and format
    //    listener must all exist, so failure aborts startup. It shares the
    //    shutdown flag every producer checks (§1.5 step 1).
    let shutting_down = Arc::new(AtomicBool::new(false));
    let clipboard = match ClipboardThread::start(
        config.settings.clipboard_max_item_bytes,
        config.settings.clipboard_source,
        config.settings.clipboard_capture,
        Wake::new(event_tx.clone(), ctx.clone(), shutting_down.clone()),
    ) {
        Ok(worker) => worker,
        Err(e) => fatal(&format!("Clipboard worker failed to start: {e}")),
    };

    // 6. Root viewport: hidden, undecorated, always-on-top palette (§2.1). The
    //    first summon makes it visible; `X` hides; tray Quit closes.
    //    Mica needs a transparent GL surface, so it is decided here (§12.7).
    let mica = appearance::mica_available();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("smolcast")
            .with_app_id("smolcast")
            .with_inner_size(theme::window_size(config.settings.palette_size))
            .with_visible(false)
            .with_decorations(false)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_transparent(mica),
        ..Default::default()
    };

    match eframe::run_native_ext(
        "smolcast",
        options,
        Some(ctx),
        Box::new(move |cc| {
            let model = AppModel::new(
                event_rx,
                config,
                usage,
                favorites,
                startup_notices,
                force_visible,
                report,
            );
            let app = EframeApp::build(
                cc,
                model,
                event_tx,
                paths,
                clipboard,
                shutting_down,
                Startup {
                    config_failed,
                    mica,
                },
            );
            Ok(Box::new(app))
        }),
    ) {
        Ok(()) => Ok(()),
        Err(e) => fatal(&format!("The main window failed to start: {e}")),
    }
}

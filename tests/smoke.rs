//! Desktop smoke test (decision 029): the real release exe on a real desktop,
//! driven with real input. It catches regressions in the paths users take
//! every day; it is not a spec and should stay short.
//!
//! It types and clicks on whatever desktop it runs on, so it only runs where
//! `SMOLCAST_SMOKE_DESKTOP=disposable` says the desktop and the user profile
//! are throwaway (CI's `windows-latest` runner). Run it with
//! `cargo test --release --target x86_64-pc-windows-msvc --test smoke -- --ignored`.
//!
//! The app is a black box: no test hooks. Each check waits on something a
//! user would see happen (the palette shows or hides, text lands in a window,
//! a launched program runs), polling with a deadline instead of sleeping.

#![cfg(windows)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MOD_ALT, MOD_CONTROL, MOUSE_EVENT_FLAGS,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, MapVirtualKeyW, RegisterHotKey,
    SendInput, UnregisterHotKey, VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_H, VK_MENU, VK_RETURN,
    VK_SHIFT, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DispatchMessageW, EnumWindows, GetForegroundWindow, GetMessageW,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    MSG, SetCursorPos, SetWindowTextW, TranslateMessage, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
    WS_VISIBLE,
};
use windows::core::{BOOL, w};

const OPT_IN: &str = "SMOLCAST_SMOKE_DESKTOP";
const WAIT: Duration = Duration::from_secs(10);
/// Lets a frame or two pass between typing and Enter, as a person would.
const SETTLE: Duration = Duration::from_millis(400);
const SHORTCUT_NAME: &str = "Smolcast Smoke Marker";
const CF_UNICODETEXT: u32 = 13;

#[test]
#[ignore = "drives the desktop; CI runs it with --ignored"]
fn smoke() {
    assert_eq!(
        std::env::var(OPT_IN).as_deref(),
        Ok("disposable"),
        "this test types and clicks on the desktop and resets smolcast's state; \
         set {OPT_IN}=disposable only on a throwaway machine"
    );

    let marker = std::env::temp_dir().join("smolcast-smoke-launched.txt");
    let _ = std::fs::remove_file(&marker);
    reset_state();
    let shortcut = create_marker_shortcut(&marker);
    let target = TargetWindow::open();
    let app = App::launch();

    step("a second launch exits silently", || {
        let mut second = KillOnDrop(spawn_smolcast());
        let status = wait_for("the second launch to exit", || second.0.try_wait().unwrap());
        assert!(status.success(), "second launch exited with {status}");
        assert!(app.is_running(), "the first instance exited");
    });

    step("the hotkey summons the palette and Esc hides it", || {
        target.focus();
        app.summon();
        key(VK_ESCAPE);
        app.wait_hidden();
        wait_until("focus to return to the target", || target.is_foreground());
    });

    step("a calculation pastes its result", || {
        target.clear();
        target.focus();
        app.summon();
        type_and_enter("6*7");
        target.wait_for_text("42");
    });

    step("clipboard history pastes a copied entry", || {
        let copied = "smolcast smoke clipboard 1729";
        set_clipboard(copied);
        target.clear();
        target.focus();
        app.summon();
        open_command("Clipboard History");
        key(VK_RETURN);
        target.wait_for_text(copied);
    });

    step("the emoji picker pastes an emoji", || {
        target.clear();
        target.focus();
        app.summon();
        open_command("Emoji & Symbols");
        type_and_enter("thumbs up");
        target.wait_for_text("👍");
    });

    step(
        "a hidden entry comes back from Manage Hidden Entries",
        || {
            let hidden = roaming_dir().join("hidden.json");
            let read = || std::fs::read_to_string(&hidden).ok();
            let lists_marker = || read().is_some_and(|text| text.contains(SHORTCUT_NAME));
            app.summon();
            type_text(SHORTCUT_NAME);
            thread::sleep(SETTLE);
            send(&[
                key_input(VK_CONTROL, false),
                key_input(VK_SHIFT, false),
                key_input(VK_H, false),
                key_input(VK_H, true),
                key_input(VK_SHIFT, true),
                key_input(VK_CONTROL, true),
            ]);
            wait_until("the shortcut to be saved as hidden", lists_marker);
            key(VK_ESCAPE);
            app.wait_hidden();
            app.summon();
            open_command("Manage Hidden Entries");
            key(VK_RETURN);
            // The file is rewritten without it, not removed.
            wait_until("the shortcut to be unhidden", || {
                read().is_some_and(|text| !text.contains(SHORTCUT_NAME))
            });
            // Back to the root list, then away.
            key(VK_ESCAPE);
            thread::sleep(SETTLE);
            key(VK_ESCAPE);
            app.wait_hidden();
        },
    );

    step("the launcher starts a Start Menu shortcut", || {
        target.focus();
        app.summon();
        type_and_enter(SHORTCUT_NAME);
        wait_until("the shortcut's program to run", || marker.exists());
        app.wait_hidden();
    });

    assert!(app.is_running(), "smolcast exited during the smoke test");
    let _ = std::fs::remove_file(shortcut);
}

/// Run one named check, so a failure says which user path broke.
/// A failure saves `smoke-failure.png` while the app and the target window
/// are still on screen.
fn step(name: &str, check: impl FnOnce()) {
    eprintln!("smoke: {name}");
    if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(check)) {
        screenshot("smoke-failure.png");
        std::panic::resume_unwind(panic);
    }
}

fn screenshot(path: &str) {
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms, System.Drawing; \
         $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds; \
         $i = New-Object System.Drawing.Bitmap $b.Width, $b.Height; \
         [System.Drawing.Graphics]::FromImage($i).CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size); \
         $i.Save('{path}')"
    );
    let _ = powershell(&script);
}

fn powershell(script: &str) -> std::io::Result<ExitStatus> {
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .status()
}

/// Type into the palette, then press Enter once the typed text has settled.
fn type_and_enter(text: &str) {
    type_text(text);
    thread::sleep(SETTLE);
    key(VK_RETURN);
}

/// Type a built-in command's name and open it from the root list.
fn open_command(name: &str) {
    type_and_enter(name);
    thread::sleep(SETTLE);
}

// ---------------------------------------------------------------------------
// The app under test

fn spawn_smolcast() -> Child {
    Command::new(env!("CARGO_BIN_EXE_smolcast"))
        .spawn()
        .unwrap()
}

struct App {
    child: RefCell<Child>,
}

impl App {
    fn launch() -> Self {
        let app = Self {
            child: RefCell::new(spawn_smolcast()),
        };
        // The hidden palette exists once eframe has started; the hotkey is
        // registered in the same callback.
        wait_until("smolcast's window to exist", || !app.windows().is_empty());
        app
    }

    fn pid(&self) -> u32 {
        self.child.borrow().id()
    }

    fn is_running(&self) -> bool {
        self.child.borrow_mut().try_wait().unwrap().is_none()
    }

    /// Press the default summon hotkey (Ctrl+Alt+Space) and wait for the
    /// palette to take the foreground. It always presses at least once: right
    /// after launch eframe briefly shows the window before the app hides it,
    /// so a visible window alone does not mean the palette was summoned. The
    /// first press can also land before the hotkey is registered, so it is
    /// pressed again every 2 s; a summon while the palette is visible does
    /// nothing, so a repeat is harmless.
    fn summon(&self) {
        let keys = [VK_CONTROL, VK_MENU, VK_SPACE];
        let mut inputs: Vec<INPUT> = keys.iter().map(|&k| key_input(k, false)).collect();
        inputs.extend(keys.iter().rev().map(|&k| key_input(k, true)));
        let deadline = Instant::now() + WAIT;
        let mut pressed: Option<Instant> = None;
        while !(self.palette_visible() && self.owns(unsafe { GetForegroundWindow() })) {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for the palette to show and take focus\n{}",
                self.report()
            );
            if pressed.is_none_or(|at| at.elapsed() > Duration::from_secs(2)) {
                send(&inputs);
                pressed = Some(Instant::now());
            }
            thread::sleep(Duration::from_millis(50));
        }
        thread::sleep(SETTLE);
    }

    fn wait_hidden(&self) {
        let deadline = Instant::now() + WAIT;
        while self.palette_visible() {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for the palette to hide\n{}",
                self.report()
            );
            thread::sleep(Duration::from_millis(50));
        }
    }

    /// The palette is the window titled "smolcast"; smolcast also owns an
    /// untitled window that stays visible. It counts only when shown and not
    /// cloaked: the summon fast path shows it cloaked until a fresh frame is
    /// ready.
    fn palette_visible(&self) -> bool {
        self.windows().into_iter().any(|hwnd| {
            window_text(hwnd) == "smolcast"
                && unsafe { IsWindowVisible(hwnd) }.as_bool()
                && !cloaked(hwnd)
        })
    }

    /// What the desktop looked like when the palette did not show or hide: smolcast's
    /// windows, the foreground window, and whether the hotkey is still free
    /// (smolcast never registered it).
    fn report(&self) -> String {
        let mut lines: Vec<String> = self
            .windows()
            .into_iter()
            .map(|hwnd| {
                format!(
                    "  smolcast window {:?} {:?}: visible {}, cloaked {}",
                    hwnd.0,
                    window_text(hwnd),
                    unsafe { IsWindowVisible(hwnd) }.as_bool(),
                    cloaked(hwnd)
                )
            })
            .collect();
        let foreground = unsafe { GetForegroundWindow() };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(foreground, Some(&mut pid)) };
        lines.push(format!(
            "  foreground {:?} {:?}, pid {pid} (smolcast is {}, this test {})",
            foreground.0,
            window_text(foreground),
            self.pid(),
            std::process::id()
        ));
        let free =
            unsafe { RegisterHotKey(None, 0x5AFE, MOD_CONTROL | MOD_ALT, VK_SPACE.0 as u32) }
                .is_ok();
        if free {
            let _ = unsafe { UnregisterHotKey(None, 0x5AFE) };
        }
        lines.push(format!("  Ctrl+Alt+Space free to register: {free}"));
        lines.push(format!("  smolcast running: {}", self.is_running()));
        lines.join("\n")
    }

    fn owns(&self, hwnd: HWND) -> bool {
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        pid == self.pid()
    }

    /// smolcast's top-level windows: the palette, plus hidden tray windows.
    fn windows(&self) -> Vec<HWND> {
        struct Search {
            pid: u32,
            found: Vec<HWND>,
        }
        unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let search = unsafe { &mut *(lparam.0 as *mut Search) };
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            if pid == search.pid {
                search.found.push(hwnd);
            }
            true.into()
        }
        let mut search = Search {
            pid: self.pid(),
            found: Vec::new(),
        };
        let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize)) };
        search.found
    }
}

struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let child = self.child.get_mut();
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Start from a first run: no config, history, usage, favorites or hidden
/// entries. The config written here only turns off what would make the run
/// slower or depend on the network or on Windows' own clipboard history.
fn reset_state() {
    let roaming = roaming_dir();
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join("smolcast");
    for dir in [&roaming, &local] {
        let _ = std::fs::remove_dir_all(dir);
    }
    std::fs::create_dir_all(&roaming).unwrap();
    std::fs::write(
        roaming.join("config.toml"),
        "schema_version = 1\n\
         [settings]\n\
         summon_hotkey = \"control+alt+Space\"\n\
         clipboard_capture = true\n\
         clipboard_max_entries = 100\n\
         clipboard_max_item_bytes = 8388608\n\
         clipboard_max_total_bytes = 67108864\n\
         clipboard_source = \"smolcast\"\n\
         clipboard_persist = false\n\
         reduce_motion = \"on\"\n\
         keep_state_secs = 0\n\
         check_for_updates = false\n",
    )
    .unwrap();
}

/// `%APPDATA%\smolcast`: config and user-curated state.
fn roaming_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("smolcast")
}

/// A per-user Start Menu shortcut whose program writes `marker`, so a launch
/// is observable without leaving a window open.
fn create_marker_shortcut(marker: &Path) -> PathBuf {
    let programs = PathBuf::from(std::env::var_os("APPDATA").unwrap())
        .join(r"Microsoft\Windows\Start Menu\Programs");
    let lnk = programs.join(format!("{SHORTCUT_NAME}.lnk"));
    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); \
         $s.TargetPath = $env:ComSpec; \
         $s.Arguments = '/c echo launched> \"{}\"'; \
         $s.Save()",
        lnk.display(),
        marker.display()
    );
    let status = powershell(&script).unwrap();
    assert!(
        status.success() && lnk.exists(),
        "could not create {}",
        lnk.display()
    );
    lnk
}

// ---------------------------------------------------------------------------
// The paste destination: a plain Win32 edit window owned by this process

struct TargetWindow {
    hwnd: isize,
}

impl TargetWindow {
    fn open() -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || unsafe {
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!("smolcast smoke target"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                40,
                40,
                480,
                160,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            tx.send(hwnd.0 as isize).unwrap();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
        Self {
            hwnd: rx.recv().unwrap(),
        }
    }

    fn hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut _)
    }

    fn is_foreground(&self) -> bool {
        (unsafe { GetForegroundWindow() }) == self.hwnd()
    }

    /// Click into the window, as a user would before summoning.
    fn focus(&self) {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(self.hwnd(), &mut rect) }.unwrap();
        unsafe { SetCursorPos((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2) }
            .unwrap();
        send(&[
            mouse_input(MOUSEEVENTF_LEFTDOWN),
            mouse_input(MOUSEEVENTF_LEFTUP),
        ]);
        wait_until("the target window to take focus", || self.is_foreground());
    }

    fn clear(&self) {
        unsafe { SetWindowTextW(self.hwnd(), w!("")) }.unwrap();
    }

    fn text(&self) -> String {
        let len = unsafe { GetWindowTextLengthW(self.hwnd()) };
        let mut buf = vec![0u16; len as usize + 1];
        let copied = unsafe { GetWindowTextW(self.hwnd(), &mut buf) };
        String::from_utf16_lossy(&buf[..copied as usize])
    }

    fn wait_for_text(&self, expected: &str) {
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            if self.text() == expected {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!(
            "expected {expected:?} pasted into the target, found {:?}",
            self.text()
        );
    }
}

// ---------------------------------------------------------------------------
// Real input and the clipboard

fn send(inputs: &[INPUT]) {
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    assert_eq!(sent as usize, inputs.len(), "SendInput was blocked");
}

fn key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let scan = unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC) } as u16;
    let flags = if up {
        KEYEVENTF_KEYUP
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    keyboard_input(vk, scan, flags)
}

/// A keyboard event: a virtual key with its scan code, or with
/// `KEYEVENTF_UNICODE`, a UTF-16 unit in `scan` (and `vk` 0).
fn keyboard_input(vk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

fn mouse_input(flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

fn key(vk: VIRTUAL_KEY) {
    send(&[key_input(vk, false), key_input(vk, true)]);
}

/// Type text as Unicode key events, one character at a time.
fn type_text(text: &str) {
    for unit in text.encode_utf16() {
        send(&[
            keyboard_input(VIRTUAL_KEY(0), unit, KEYEVENTF_UNICODE),
            keyboard_input(VIRTUAL_KEY(0), unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
        ]);
        thread::sleep(Duration::from_millis(15));
    }
}

/// Copy text the way any app does; smolcast's listener captures it.
fn set_clipboard(text: &str) {
    let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2).unwrap();
        let dst = GlobalLock(memory) as *mut u16;
        std::ptr::copy_nonoverlapping(units.as_ptr(), dst, units.len());
        let _ = GlobalUnlock(memory);
        // smolcast may still be reading the previous copy.
        wait_until("the clipboard to open", || OpenClipboard(None).is_ok());
        EmptyClipboard().unwrap();
        SetClipboardData(CF_UNICODETEXT, Some(HANDLE(memory.0))).unwrap();
        CloseClipboard().unwrap();
    }
    // The capture is asynchronous and has no visible sign; give it a moment.
    thread::sleep(Duration::from_millis(500));
}

fn cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    let _ = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
    cloaked != 0
}

fn window_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

// ---------------------------------------------------------------------------
// Waiting

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    wait_for(what, || done().then_some(()));
}

fn wait_for<T>(what: &str, mut poll: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(value) = poll() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(50));
    }
}

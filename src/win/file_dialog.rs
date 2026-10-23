//! The standard Open dialog for tracking a script file (plan §10.3).
//!
//! Runs on its own thread inside a single-threaded apartment, like the shell
//! jobs, owned by the palette window so it stays in front of it. The result
//! reaches the model as `AppEvent::ScriptFilePicked`.

use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FileOpenDialog, IFileOpenDialog,
    SIGDN_FILESYSPATH,
};
use windows::core::{HRESULT, PCWSTR};

use crate::app::model::{AppEvent, Wake};
use crate::core::script_file::EXTENSIONS;

use super::shell::ComApartment;
use super::wide;

/// Show the dialog off the UI thread. `owner` is the palette's HWND as an
/// integer (HWND is not `Send`).
pub fn spawn_pick_script(owner: Option<isize>, wake: Wake) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("smolcast-file-dialog".into())
        .spawn(move || {
            // Always answer, even if the dialog code panics: the model keeps
            // click-away dismissal off until this event arrives.
            let owner = owner.map(|h| HWND(h as *mut _));
            let result = std::panic::catch_unwind(|| pick_script(owner))
                .unwrap_or_else(|_| Err("The file dialog failed.".to_owned()));
            wake.send(AppEvent::ScriptFilePicked(result));
        })
        .map(drop)
}

/// The chosen file's full path, or `None` when the dialog was cancelled.
fn pick_script(owner: Option<HWND>) -> Result<Option<String>, String> {
    let _com = ComApartment::enter().map_err(|e| e.to_string())?;
    let scripts = EXTENSIONS.map(|e| format!("*.{e}")).join(";");
    let (scripts_name, scripts_spec) = (wide("Scripts and programs"), wide(&scripts));
    let (all_name, all_spec) = (wide("All files"), wide("*.*"));
    let title = wide("Track a script file");
    let filters = [
        COMDLG_FILTERSPEC {
            pszName: PCWSTR(scripts_name.as_ptr()),
            pszSpec: PCWSTR(scripts_spec.as_ptr()),
        },
        COMDLG_FILTERSPEC {
            pszName: PCWSTR(all_name.as_ptr()),
            pszSpec: PCWSTR(all_spec.as_ptr()),
        },
    ];
    let open_failed = |e: windows::core::Error| format!("Could not open the file dialog: {e}");
    let read_failed = |e: windows::core::Error| format!("Could not read the chosen file: {e}");
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).map_err(open_failed)?;
        let options = dialog.GetOptions().map_err(open_failed)?;
        dialog
            // NOCHANGEDIR: browsing must not move smolcast's own working folder.
            .SetOptions(options | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM | FOS_NOCHANGEDIR)
            .and_then(|()| dialog.SetFileTypes(&filters))
            .and_then(|()| dialog.SetTitle(PCWSTR(title.as_ptr())))
            .map_err(open_failed)?;
        match dialog.Show(owner) {
            Ok(()) => {}
            Err(e) if e.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => return Ok(None),
            Err(e) => return Err(format!("The file dialog failed: {e}")),
        }
        let item = dialog.GetResult().map_err(read_failed)?;
        let name = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(read_failed)?;
        let path = name.to_string();
        CoTaskMemFree(Some(name.0 as *const _));
        path.map(Some)
            .map_err(|_| "The chosen file's path is not valid Unicode.".to_owned())
    }
}

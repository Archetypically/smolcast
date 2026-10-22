//! System actions from the palette root: lock, sleep, restart, shut down,
//! sign out, empty the Recycle Bin, dark mode, mute, and display off.
//!
//! Each runs on the launch thread (`shell::spawn_launch`) and returns once
//! Windows accepted it. Restart, shut down and sign out ask apps to close
//! normally (no force flag), so an app with unsaved work can still object.

use windows::Win32::Foundation::{
    CloseHandle, ERROR_SUCCESS, HANDLE, LPARAM, LUID, WIN32_ERROR, WPARAM,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
    SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::System::Shutdown::{
    EWX_HYBRID_SHUTDOWN, EWX_LOGOFF, EWX_POWEROFF, EWX_REBOOT, EXIT_WINDOWS_FLAGS, ExitWindowsEx,
    LockWorkStation, SHTDN_REASON_FLAG_PLANNED, SHTDN_REASON_MAJOR_OTHER, SHTDN_REASON_MINOR_OTHER,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY, VK_VOLUME_MUTE,
};
use windows::Win32::UI::Shell::{
    SHERB_NOCONFIRMATION, SHEmptyRecycleBinW, SHQUERYRBINFO, SHQueryRecycleBinW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, PostMessageW, SC_MONITORPOWER, SMTO_ABORTIFHUNG, SendMessageTimeoutW,
    WM_SETTINGCHANGE, WM_SYSCOMMAND,
};
use windows::core::{PCWSTR, w};

use crate::core::app_index::SystemAction;
use crate::win::shell::LaunchError;

const PERSONALIZE: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");

pub fn run(action: SystemAction) -> Result<(), LaunchError> {
    let operation = action.title();
    let fail = |e: windows::core::Error| LaunchError::from_win(operation, &e);
    match action {
        SystemAction::LockScreen => unsafe { LockWorkStation() }.map_err(fail),
        SystemAction::Sleep => {
            // Not hibernate; wake events stay enabled.
            if unsafe { SetSuspendState(false, false, false) } {
                Ok(())
            } else {
                Err(fail(windows::core::Error::from_thread()))
            }
        }
        SystemAction::Restart => exit_windows(EWX_REBOOT).map_err(fail),
        // Hybrid, like the Start menu: fast startup applies when it is on.
        SystemAction::ShutDown => exit_windows(EWX_POWEROFF | EWX_HYBRID_SHUTDOWN).map_err(fail),
        SystemAction::SignOut => exit_windows(EWX_LOGOFF).map_err(fail),
        SystemAction::EmptyRecycleBin => empty_recycle_bin().map_err(fail),
        SystemAction::ToggleDarkMode => toggle_dark_mode().map_err(fail),
        SystemAction::ToggleMute => press_key(VK_VOLUME_MUTE).map_err(fail),
        SystemAction::TurnOffDisplay => {
            // 2 = power off. Posted, so a hung window cannot stall the job.
            unsafe {
                PostMessageW(
                    Some(HWND_BROADCAST),
                    WM_SYSCOMMAND,
                    WPARAM(SC_MONITORPOWER as usize),
                    LPARAM(2),
                )
            }
            .map_err(fail)
        }
    }
}

/// Restart, shut down and power off need the shutdown privilege enabled in
/// this process's token; signing out does not, but enabling it is harmless.
fn exit_windows(flags: EXIT_WINDOWS_FLAGS) -> windows::core::Result<()> {
    enable_shutdown_privilege()?;
    let reason = SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_MINOR_OTHER | SHTDN_REASON_FLAG_PLANNED;
    unsafe { ExitWindowsEx(flags, reason) }
}

fn enable_shutdown_privilege() -> windows::core::Result<()> {
    let mut token = HANDLE::default();
    unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )?;
    }
    let result = (|| {
        let mut luid = LUID::default();
        unsafe { LookupPrivilegeValueW(PCWSTR::null(), SE_SHUTDOWN_NAME, &mut luid)? };
        let privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        unsafe { AdjustTokenPrivileges(token, false, Some(&privileges), 0, None, None) }
    })();
    let _ = unsafe { CloseHandle(token) };
    result
}

/// The palette already asked for confirmation, so Windows does not ask
/// again. An empty bin is a success: Windows reports an error for it.
fn empty_recycle_bin() -> windows::core::Result<()> {
    let mut info = SHQUERYRBINFO {
        cbSize: size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    if unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) }.is_ok() && info.i64NumItems == 0 {
        return Ok(());
    }
    unsafe { SHEmptyRecycleBinW(None, PCWSTR::null(), SHERB_NOCONFIRMATION) }
}

/// Flip both "Choose your mode" switches (apps and Windows) to the opposite
/// of the apps' current mode, then tell running apps the theme changed.
fn toggle_dark_mode() -> windows::core::Result<()> {
    let light = read_dword(w!("AppsUseLightTheme")).unwrap_or(1) != 0;
    let value = u32::from(!light);
    for name in [w!("AppsUseLightTheme"), w!("SystemUsesLightTheme")] {
        write_dword(name, value).ok()?;
    }
    let area = w!("ImmersiveColorSet");
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(area.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            1000,
            None,
        )
    };
    Ok(())
}

fn read_dword(name: PCWSTR) -> Option<u32> {
    let mut value = 0u32;
    let mut bytes = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PERSONALIZE,
            name,
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut bytes),
        )
    };
    (status == ERROR_SUCCESS).then_some(value)
}

fn write_dword(name: PCWSTR, value: u32) -> WIN32_ERROR {
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PERSONALIZE,
            name,
            REG_DWORD.0,
            Some((&value as *const u32).cast()),
            size_of::<u32>() as u32,
        )
    }
}

/// Press and release one key; the volume keys show Windows' own flyout.
fn press_key(vk: VIRTUAL_KEY) -> windows::core::Result<()> {
    let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
    if unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) } == inputs.len() as u32 {
        Ok(())
    } else {
        Err(windows::core::Error::from_thread())
    }
}

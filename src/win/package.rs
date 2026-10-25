//! Package identity (plan §15): whether this process runs from the MSIX
//! package the Microsoft Store installs, and whether the package's startup
//! task started it. The same exe ships in both forms; the Store build turns
//! off self-update and registers at sign-in through its startup task.

use std::sync::OnceLock;

use windows::ApplicationModel::Activation::ActivationKind;
use windows::ApplicationModel::AppInstance;
use windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

/// Whether smolcast runs with package identity. A process without one gets
/// `APPMODEL_ERROR_NO_PACKAGE`; with one, the empty buffer is too small.
pub fn is_packaged() -> bool {
    static PACKAGED: OnceLock<bool> = OnceLock::new();
    *PACKAGED.get_or_init(|| {
        let mut length = 0u32;
        let status = unsafe { GetCurrentPackageFullName(&mut length, None) };
        status == ERROR_INSUFFICIENT_BUFFER
    })
}

/// Whether the package's `windows.startupTask` extension started this
/// process at sign-in (the packaged counterpart of the Run entry's flag).
pub fn started_at_login() -> bool {
    is_packaged()
        && AppInstance::GetActivatedEventArgs()
            .and_then(|args| args.Kind())
            .is_ok_and(|kind| kind == ActivationKind::StartupTask)
}

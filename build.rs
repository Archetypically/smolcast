//! Embeds the Windows resources in smolcast.exe: the app icon (resource 1,
//! also the tray icon) and the VERSIONINFO block Explorer shows under
//! Properties > Details. Version, author, site and description come from
//! Cargo.toml, so a release tag's version is the file's version. Host (Linux)
//! builds of the library skip this.
//!
//! MSVC builds also restrict where the exe's imported DLLs load from
//! (plan §0): see `dll_search_system32_only`.

fn main() {
    println!("cargo:rerun-if-changed=assets/smolcast.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    dll_search_system32_only();
    let author = env!("CARGO_PKG_AUTHORS").replace(':', ", ");
    let site = env!("CARGO_PKG_HOMEPAGE");
    // Details shows Copyright but not Company or Comments, so the site rides
    // in the copyright line too.
    let display_site = site.trim_start_matches("https://");
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/smolcast.ico")
        .set("ProductName", "smolcast")
        .set("FileDescription", "smolcast")
        .set("CompanyName", &author)
        .set(
            "LegalCopyright",
            &format!("© 2026 {author} ({display_site}). MIT License."),
        )
        .set("OriginalFilename", "smolcast.exe")
        .set("InternalName", "smolcast")
        .set("Comments", env!("CARGO_PKG_DESCRIPTION"))
        .set("Homepage", site);
    if let Err(e) = res.compile() {
        panic!("could not embed Windows resources: {e}");
    }
}

/// Load the DLLs smolcast.exe imports from System32 only
/// (`LOAD_LIBRARY_SEARCH_SYSTEM32`). By default Windows looks in the exe's
/// folder first, so a `winhttp.dll` or `opengl32.dll` downloaded next to
/// smolcast.exe would load into it. MSVC's linker only; the GNU target is
/// for development. The `app-folder-dlls` feature keeps the default for CI.
fn dll_search_system32_only() {
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if msvc && std::env::var_os("CARGO_FEATURE_APP_FOLDER_DLLS").is_none() {
        println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
    }
}

//! Installed fonts through DirectWrite's system collection, which covers
//! fonts installed for all users and per user (plan §12.7). Used by the
//! `font` setting: the settings form lists the families, and the host loads
//! the chosen one's regular face for egui.

use std::ffi::c_void;

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection,
    IDWriteFontFile, IDWriteLocalFontFileLoader, IDWriteLocalizedStrings,
};
use windows::core::{BOOL, Interface, Result, w};

use crate::win::{from_wide_nul, wide};

/// A font face's file contents and its index within the file (`.ttc`
/// collections hold several).
pub struct FontFile {
    pub bytes: Vec<u8>,
    pub index: u32,
}

/// Every installed family's name, sorted ignoring case. Empty if DirectWrite
/// fails.
pub fn families() -> Vec<String> {
    let mut names = family_names().unwrap_or_default();
    names.sort_by_cached_key(|name| name.to_lowercase());
    names.dedup();
    names
}

/// The regular face of the family named `family` (any of its localized
/// names), or `None` if it is not installed or its file is unreadable.
pub fn load(family: &str) -> Option<FontFile> {
    load_face(family).ok().flatten()
}

fn system_collection() -> Result<IDWriteFontCollection> {
    unsafe {
        let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let mut collection = None;
        factory.GetSystemFontCollection(&mut collection, false)?;
        collection.ok_or_else(windows::core::Error::empty)
    }
}

fn family_names() -> Result<Vec<String>> {
    let collection = system_collection()?;
    let count = unsafe { collection.GetFontFamilyCount() };
    let mut names = Vec::with_capacity(count as usize);
    for index in 0..count {
        let family = unsafe { collection.GetFontFamily(index)? };
        let localized = unsafe { family.GetFamilyNames()? };
        if let Some(name) = english_name(&localized)? {
            names.push(name);
        }
    }
    Ok(names)
}

/// The en-us name, or the first one; what the config file stores.
fn english_name(names: &IDWriteLocalizedStrings) -> Result<Option<String>> {
    let mut index = 0;
    let mut exists = BOOL(0);
    unsafe { names.FindLocaleName(w!("en-us"), &mut index, &mut exists)? };
    if !exists.as_bool() {
        if unsafe { names.GetCount() } == 0 {
            return Ok(None);
        }
        index = 0;
    }
    let len = unsafe { names.GetStringLength(index)? };
    let mut buffer = vec![0u16; len as usize + 1];
    unsafe { names.GetString(index, &mut buffer)? };
    Ok(Some(from_wide_nul(&buffer)))
}

fn load_face(family: &str) -> Result<Option<FontFile>> {
    let collection = system_collection()?;
    let name = wide(family);
    let mut index = 0;
    let mut exists = BOOL(0);
    unsafe {
        collection.FindFamilyName(
            windows::core::PCWSTR(name.as_ptr()),
            &mut index,
            &mut exists,
        )?
    };
    if !exists.as_bool() {
        return Ok(None);
    }
    let face = unsafe {
        collection
            .GetFontFamily(index)?
            .GetFirstMatchingFont(
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
            )?
            .CreateFontFace()?
    };
    let mut count = 0;
    unsafe { face.GetFiles(&mut count, None)? };
    if count != 1 {
        // A face split across files cannot be handed to egui as one buffer.
        return Ok(None);
    }
    let mut files: [Option<IDWriteFontFile>; 1] = [None];
    unsafe { face.GetFiles(&mut count, Some(files.as_mut_ptr()))? };
    let Some(file) = files[0].take() else {
        return Ok(None);
    };
    let Some(path) = local_path(&file)? else {
        return Ok(None);
    };
    Ok(std::fs::read(path).ok().map(|bytes| FontFile {
        bytes,
        index: unsafe { face.GetIndex() },
    }))
}

/// The file's path on disk, or `None` for a font not loaded from a local
/// file (a remote or in-memory loader).
fn local_path(file: &IDWriteFontFile) -> Result<Option<String>> {
    let mut key: *mut c_void = std::ptr::null_mut();
    let mut key_size = 0;
    unsafe { file.GetReferenceKey(&mut key, &mut key_size)? };
    let Ok(loader) = unsafe { file.GetLoader()? }.cast::<IDWriteLocalFontFileLoader>() else {
        return Ok(None);
    };
    let len = unsafe { loader.GetFilePathLengthFromKey(key, key_size)? };
    let mut buffer = vec![0u16; len as usize + 1];
    unsafe { loader.GetFilePathFromKey(key, key_size, &mut buffer)? };
    Ok(Some(from_wide_nul(&buffer)))
}

//! iOS side of the ROM import (the BYOR build, `native/IOS_IMPORT.md`): document picker, data dir, keep-awake. The import itself is `byor-rs`. The host functions live in
//! `ios/import/jm_import.m` and are looked up with `dlsym`, so the normal (full-assets) iOS target, which does not link that
//! file, builds and runs exactly as before: `available()` is simply false there.
use super::*;
use std::ffi::{c_char, c_int, c_void, CString};

unsafe extern "C" {
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

fn lookup(name: &str) -> Option<*mut c_void> {
    let c = CString::new(name).ok()?;
    let p = unsafe { dlsym(RTLD_DEFAULT, c.as_ptr()) };
    (!p.is_null()).then_some(p)
}

/// Whether this binary carries the import host (i.e. it is the BYOR target).
pub fn available() -> bool {
    lookup("jm_pick_roms").is_some()
}

fn call0(name: &str) {
    if let Some(p) = lookup(name) {
        let f: unsafe extern "C" fn() = unsafe { std::mem::transmute(p) };
        unsafe { f() }
    }
}

/// No auto-lock + a background-task assertion while an extraction runs.
pub fn keep_awake(on: bool) {
    if let Some(p) = lookup("jm_keep_awake") {
        let f: unsafe extern "C" fn(c_int) = unsafe { std::mem::transmute(p) };
        unsafe { f(on as c_int) }
    }
}

/// Shows the system document picker; chosen files appear through `picked_pop` once copied into the sandbox.
pub fn pick_roms() {
    call0("jm_pick_roms");
}

pub fn picked_pop() -> Option<PathBuf> {
    let p = lookup("jm_picked_pop")?;
    let f: unsafe extern "C" fn(*mut c_char, usize) -> c_int = unsafe { std::mem::transmute(p) };
    let mut buf = vec![0u8; 2048];
    if unsafe { f(buf.as_mut_ptr().cast(), buf.len()) } == 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(PathBuf::from(String::from_utf8_lossy(&buf[..end]).into_owned()))
}

/// Creates the data dir and excludes it from iCloud / device backups (it is regenerable from the ROMs).
pub fn prepare_data_dir() {
    if let Some(p) = lookup("jm_prepare_data_dir") {
        let f: unsafe extern "C" fn() -> *const c_char = unsafe { std::mem::transmute(p) };
        unsafe { f() };
    }
}

pub fn documents_dir() -> PathBuf {
    let from_host = lookup("jm_documents_dir").map(|p| {
        let f: unsafe extern "C" fn() -> *const c_char = unsafe { std::mem::transmute(p) };
        unsafe { std::ffi::CStr::from_ptr(f()) }.to_string_lossy().into_owned()
    });
    from_host.map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Documents"))
}

/// Files that could be N64 ROM dumps (4-80 MB) directly inside `dir`.
pub fn rom_candidates(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.metadata().is_ok_and(|m| m.is_file() && (4 << 20..=80 << 20).contains(&m.len())))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

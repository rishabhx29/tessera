//! Which ONNX Runtime the loader actually bound, on Windows.
//!
//! A platform adapter behind the same seam as the OCR engines: the DLL search
//! order is a Windows fact with no portable equivalent, and it is the fact
//! behind the one ONNX failure `ort` cannot explain — a build whose bundled
//! 1.23 `onnxruntime.dll` is missing silently binds to the older copy Windows
//! keeps in `System32`, and every ONNX call then dies on an API-level check.
//! Reading the loaded version is what turns that into a fixable sentence.
//!
//! Everything here is read-only and best-effort: the answer feeds a message, so
//! a version that cannot be read is `None` rather than an error. On every other
//! platform there is no search order to disambiguate — the build names one path —
//! and this returns `None`.

/// The version string of the ONNX Runtime this process actually loaded, or
/// `None` when it cannot be read.
///
/// Reads the same export `ort` reads — `OrtGetApiBase` -> `GetVersionString` —
/// from the module already mapped into this process, so it answers "which
/// onnxruntime.dll did the loader pick?" rather than "which one was installed?".
/// It goes through the raw module handle rather than `ort_sys`'s linked symbol
/// on purpose: the symbol is the thing that failed, and this has to be able to
/// speak when it did.
#[cfg(windows)]
pub(crate) fn loaded_runtime_version() -> Option<String> {
    use windows::core::{PCSTR, PCWSTR};
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

    /// `ort_sys::OrtApiBase`'s C layout: `GetApi` then `GetVersionString`.
    #[repr(C)]
    struct OrtApiBase {
        _get_api: Option<unsafe extern "system" fn(u32) -> *const core::ffi::c_void>,
        get_version_string: Option<unsafe extern "system" fn() -> *const core::ffi::c_char>,
    }

    let name: Vec<u16> = "onnxruntime.dll"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let module: HMODULE = unsafe { GetModuleHandleW(PCWSTR(name.as_ptr())) }.ok()?;
    let get_base = unsafe { GetProcAddress(module, PCSTR(c"OrtGetApiBase".as_ptr().cast())) }?;
    let get_base: unsafe extern "system" fn() -> *const OrtApiBase =
        unsafe { std::mem::transmute(get_base) };
    let base = unsafe { get_base() };
    if base.is_null() {
        return None;
    }
    let get_version = unsafe { (*base).get_version_string }?;
    let version = unsafe { get_version() };
    if version.is_null() {
        return None;
    }
    Some(
        unsafe { std::ffi::CStr::from_ptr(version) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// No second copy of the runtime exists on the search path anywhere else, so
/// there is nothing to disambiguate.
#[cfg(not(windows))]
pub(crate) fn loaded_runtime_version() -> Option<String> {
    None
}

//! Runtime acquisition of the VapourSynth API function table.

use std::ffi::c_int;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::LazyLock;

use libloading::Library;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use vapoursynth4_rs::api::Api;
use vapoursynth4_rs::ffi;

type GetVapourSynthApi = unsafe extern "system-unwind" fn(version: c_int) -> *const ffi::VSAPI;

/// File name of the VapourSynth shared library inside the `vapoursynth` wheel.
#[cfg(target_os = "windows")]
const VS_LIBRARY: &str = "libvapoursynth.dll";
#[cfg(target_os = "macos")]
const VS_LIBRARY: &str = "libvapoursynth.4.dylib";
#[cfg(all(unix, not(target_os = "macos")))]
const VS_LIBRARY: &str = "libvapoursynth.so.4";

pub(crate) struct Apis {
  pub(crate) api: Api,
  // Keeps the VapourSynth shared library mapped for the lifetime of the
  // process.
  _lib: Library,
}

// SAFETY: `Api` is a raw pointer to an immutable, process-lifetime
// function-pointer table inside the loaded library.
unsafe impl Send for Apis {}
// SAFETY: same as `Send` above. The table is never mutated.
unsafe impl Sync for Apis {}

/// Resolves the path to the VapourSynth shared library bundled with the
/// installed `vapoursynth` Python package.
fn vapoursynth_library_path() -> PyResult<PathBuf> {
  Python::attach(|py| {
    let module = py.import("vapoursynth")?;
    let file: String = module.getattr("__file__")?.extract()?;
    let dir = PathBuf::from(file)
      .parent()
      .ok_or_else(|| PyRuntimeError::new_err("vapoursynth.__file__ has no parent directory"))?
      .to_path_buf();
    Ok(dir.join(VS_LIBRARY))
  })
}

static APIS: LazyLock<Result<Apis, String>> = LazyLock::new(|| {
  let path = vapoursynth_library_path().map_err(|e| e.to_string())?;
  // SAFETY: opening the VapourSynth shared library has no preconditions that
  // are visible from here.
  let lib = unsafe { Library::new(&path) }.map_err(|e| {
    format!(
      "failed to load VapourSynth library at {}: {e}",
      path.display()
    )
  })?;
  // SAFETY: `getVapourSynthAPI` is exported by `libvapoursynth` with exactly
  // this signature.
  let get_api = unsafe { lib.get::<GetVapourSynthApi>(b"getVapourSynthAPI\0") }
    .map_err(|e| format!("VapourSynth library is missing getVapourSynthAPI: {e}"))?;
  // SAFETY: FFI call with no preconditions. Null is handled by `NonNull::new`.
  let ptr = unsafe { get_api(ffi::VAPOURSYNTH_API_VERSION) };
  let Some(api_ptr) = NonNull::new(ptr.cast_mut()) else {
    return Err(format!(
      "the VapourSynth library does not support API {}.{}",
      ffi::VAPOURSYNTH_API_MAJOR,
      ffi::VAPOURSYNTH_API_MINOR
    ));
  };
  // SAFETY: `Api` is #[repr(transparent)] over `*const VSAPI`, and `NonNull`
  // is #[repr(transparent)] over `*const VSAPI`.
  let api: Api = unsafe { std::mem::transmute(api_ptr) };
  Ok(Apis { api, _lib: lib })
});

pub(crate) fn apis() -> PyResult<&'static Apis> {
  APIS
    .as_ref()
    .map_err(|e| PyRuntimeError::new_err(e.clone()))
}

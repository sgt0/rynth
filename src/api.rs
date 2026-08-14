//! Runtime acquisition of the VapourSynth API function table.

// TODO: just how hacky is this?

use std::ffi::c_int;
use std::ptr::NonNull;
use std::sync::LazyLock;

use pyo3::PyResult;
use pyo3::exceptions::PyRuntimeError;
use vapoursynth4_rs::api::Api;
use vapoursynth4_rs::ffi;

unsafe extern "system-unwind" {
  fn getVapourSynthAPI(version: c_int) -> *const ffi::VSAPI;
}

pub(crate) struct Apis {
  pub(crate) api: Api,
}

// SAFETY: `Api` is a raw pointer to an immutable, process-lifetime
// function-pointer table inside the linked library.
unsafe impl Send for Apis {}
// SAFETY: same as `Send` above. The table is never mutated.
unsafe impl Sync for Apis {}

static APIS: LazyLock<Result<Apis, String>> = LazyLock::new(|| {
  // SAFETY: FFI call with no preconditions. Null is handled by `NonNull::new`.
  let ptr = unsafe { getVapourSynthAPI(ffi::VAPOURSYNTH_API_VERSION) };
  let Some(api_ptr) = NonNull::new(ptr.cast_mut()) else {
    return Err(format!(
      "the linked VapourSynth library does not support API {}.{}",
      ffi::VAPOURSYNTH_API_MAJOR,
      ffi::VAPOURSYNTH_API_MINOR
    ));
  };
  // SAFETY: `Api` is #[repr(transparent)] over `*const VSAPI`, and `NonNull`
  // is #[repr(transparent)] over `*const VSAPI`.
  let api: Api = unsafe { std::mem::transmute(api_ptr) };
  Ok(Apis { api })
});

pub(crate) fn apis() -> PyResult<&'static Apis> {
  APIS
    .as_ref()
    .map_err(|e| PyRuntimeError::new_err(e.clone()))
}

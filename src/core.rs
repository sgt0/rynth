//! The `Core` pyclass and the lazy `core` proxy.

use std::ffi::CString;
use std::sync::Arc;

use pyo3::exceptions::{PyAttributeError, PyValueError};
use pyo3::prelude::*;
use vapoursynth4_rs::core::Core;

use crate::api::apis;
use crate::plugin::PyPlugin;

/// Keeps the VS core alive for as long as any node/frame derived from it
/// exists.
pub(crate) struct OwnerCell(Core);

// SAFETY: It is safe to use a core from multiple threads. We only issue
// thread-safe VSAPI calls through them.
#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for OwnerCell {}
// SAFETY: same as `Send` above.
unsafe impl Sync for OwnerCell {}

impl OwnerCell {
  pub(crate) fn with_core<R>(&self, f: impl FnOnce(&Core) -> R) -> R {
    f(&self.0)
  }
}

#[pyclass(name = "Core", frozen)]
pub(crate) struct PyCore {
  pub(crate) owner: Arc<OwnerCell>,
}

#[pymethods]
impl PyCore {
  #[new]
  #[pyo3(signature = (threads=None, max_cache_size=None))]
  pub(crate) fn new(threads: Option<i32>, max_cache_size: Option<i64>) -> PyResult<Self> {
    let apis = apis()?;
    // `max_cache_size` is in mebibytes, matching the `max_cache_size` property.
    let core = Core::builder()
      .api(apis.api)
      .maybe_thread_count(threads)
      .maybe_max_cache_size(max_cache_size.map(|mb| mb * 1024 * 1024))
      .build();
    Ok(Self {
      owner: Arc::new(OwnerCell(core)),
    })
  }

  /// Namespaces of all loaded plugins.
  #[getter]
  fn plugins(&self) -> Vec<(String, String, String)> {
    self.owner.with_core(|core| {
      core
        .plugins()
        .map(|p| {
          (
            p.namespace().to_string_lossy().into_owned(),
            p.id().to_string_lossy().into_owned(),
            p.name().to_string_lossy().into_owned(),
          )
        })
        .collect()
    })
  }

  /// The number of concurrent threads used by the core.
  #[getter]
  fn num_threads(&self) -> i32 {
    self.owner.with_core(|core| core.get_info().num_threads)
  }

  #[setter]
  fn set_num_threads(&self, value: i32) -> PyResult<()> {
    if value < 0 {
      return Err(PyValueError::new_err(
        "Number of threads must not be negative",
      ));
    }
    let api = apis()?.api;
    // SAFETY: `setThreadCount` is a thread-safe VSAPI call, and `api` is the
    // same function table the core was built with.
    self.owner.with_core(|core| unsafe {
      (api.setThreadCount)(value, core.as_ptr());
    });
    Ok(())
  }

  /// The upper framebuffer cache size, rounded up to whole mebibytes (MiB),
  /// after which memory is aggressively freed.
  #[getter]
  fn max_cache_size(&self) -> i64 {
    let bytes = self
      .owner
      .with_core(|core| core.get_info().max_framebuffer_size);
    (bytes + 1024 * 1024 - 1) / (1024 * 1024)
  }

  #[setter]
  fn set_max_cache_size(&self, mb: i64) -> PyResult<()> {
    if mb <= 0 {
      return Err(PyValueError::new_err(
        "Maximum cache size must be a positive number",
      ));
    }
    let api = apis()?.api;
    let bytes = mb * 1024 * 1024;
    // SAFETY: `setMaxCacheSize` is a thread-safe VSAPI call, and `api` is the
    // same function table the core was built with.
    self.owner.with_core(|core| unsafe {
      (api.setMaxCacheSize)(bytes, core.as_ptr());
    });
    Ok(())
  }

  fn __getattr__(&self, name: &str) -> PyResult<PyPlugin> {
    let ns = CString::new(name)
      .map_err(|_| PyAttributeError::new_err(format!("invalid namespace {name:?}")))?;
    let found = self
      .owner
      .with_core(|core| core.get_plugin_by_namespace(&ns).is_some());
    if !found {
      return Err(PyAttributeError::new_err(format!(
        "no plugin with namespace '{name}'"
      )));
    }
    Ok(PyPlugin {
      owner: self.owner.clone(),
      namespace: ns,
    })
  }
}

/// A lazy proxy for the current environment's core.
#[pyclass(name = "CoreProxy", frozen)]
pub(crate) struct PyCoreProxy;

#[pymethods]
impl PyCoreProxy {
  /// The real `Core` object behind the proxy.
  #[getter]
  #[allow(clippy::unused_self)] // pyo3 getters must be instance methods
  fn core(&self, py: Python<'_>) -> PyResult<Py<PyCore>> {
    crate::environment::current_core(py)
  }

  #[allow(clippy::needless_pass_by_value)] // pyo3 __getattr__ takes PyRef by value
  fn __getattr__<'py>(slf: PyRef<'py, Self>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    let py = slf.py();
    crate::environment::current_core(py)?.bind(py).getattr(name)
  }

  #[allow(clippy::unused_self)] // __repr__ must be an instance method
  fn __repr__(&self, py: Python<'_>) -> &'static str {
    if crate::environment::core_resolved(py) {
      "<rynth.CoreProxy (resolved)>"
    } else {
      "<rynth.CoreProxy (unresolved)>"
    }
  }
}

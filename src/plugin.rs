//! Plugin namespaces and their callable functions.

use std::ffi::CString;
use std::sync::Arc;

use pyo3::exceptions::{PyAttributeError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use vapoursynth4_rs::map::KeyStr;

use crate::convert::{key_to_py, kwargs_to_map, map_to_py_dict, signature_types};
use crate::core::OwnerCell;

#[pyclass(name = "Plugin", frozen)]
pub(crate) struct PyPlugin {
  pub(crate) owner: Arc<OwnerCell>,
  pub(crate) namespace: CString,
}

#[pymethods]
impl PyPlugin {
  #[getter]
  fn namespace(&self) -> String {
    self.namespace.to_string_lossy().into_owned()
  }

  /// Names of all functions in this plugin.
  #[getter]
  fn functions(&self) -> Vec<String> {
    self.owner.with_core(|core| {
      let plugin = core
        .get_plugin_by_namespace(&self.namespace)
        .expect("validated at construction");
      plugin
        .functions()
        .map(|f| f.name().to_string_lossy().into_owned())
        .collect()
    })
  }

  fn __getattr__(&self, name: &str) -> PyResult<PyFunction> {
    let fname = CString::new(name)
      .map_err(|_| PyAttributeError::new_err(format!("invalid function name {name:?}")))?;
    let found = self.owner.with_core(|core| {
      let plugin = core
        .get_plugin_by_namespace(&self.namespace)
        .expect("validated at construction");
      plugin.get_function_by_name(&fname).is_some()
    });
    if !found {
      return Err(PyAttributeError::new_err(format!(
        "plugin '{}' has no function '{name}'",
        self.namespace.to_string_lossy()
      )));
    }
    Ok(PyFunction {
      owner: self.owner.clone(),
      namespace: self.namespace.clone(),
      name: fname,
    })
  }
}

/// Function is a simple wrapper class for a function provided by a VapourSynth
/// plugin. Its main purpose is to be called and nothing else.
#[pyclass(name = "Function", frozen)]
pub(crate) struct PyFunction {
  owner: Arc<OwnerCell>,
  namespace: CString,
  /// The function name. Identical to the string used to register the function.
  name: CString,
}

#[pymethods]
impl PyFunction {
  #[pyo3(signature = (**kwargs))]
  fn __call__(&self, py: Python<'_>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Py<PyAny>> {
    let mut args = self
      .owner
      .with_core(vapoursynth4_rs::core::Core::create_map);
    if let Some(kwargs) = kwargs {
      let types = self.owner.with_core(|core| {
        let plugin = core
          .get_plugin_by_namespace(&self.namespace)
          .expect("validated at construction");
        let func = plugin
          .get_function_by_name(&self.name)
          .expect("validated at construction");
        signature_types(&func.arguments().to_string_lossy())
      });
      kwargs_to_map(&mut args, kwargs, &types)?;
    }
    let ret = self.owner.with_core(|core| {
      let plugin = core
        .get_plugin_by_namespace(&self.namespace)
        .expect("validated at construction");
      plugin.invoke(&self.name, &args)
    });
    if let Some(err) = ret.get_error() {
      return Err(PyRuntimeError::new_err(err.to_string_lossy().into_owned()));
    }
    // Convention is that a single-key result unwraps to its value.
    if ret.len() == 1 {
      let key: CString = (**ret.get_key(0)).into();
      key_to_py(py, &ret, KeyStr::from_cstr(&key), &self.owner)
    } else {
      Ok(map_to_py_dict(py, &ret, &self.owner)?.into_any())
    }
  }

  fn __repr__(&self) -> String {
    format!(
      "<rynth.Function {}.{}>",
      self.namespace.to_string_lossy(),
      self.name.to_string_lossy()
    )
  }
}

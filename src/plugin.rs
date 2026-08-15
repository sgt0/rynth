//! Plugin namespaces and their callable functions.

use std::ffi::CString;
use std::sync::Arc;

use pyo3::exceptions::{PyAttributeError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use vapoursynth4_rs::map::KeyStr;

use crate::convert::{key_to_py, kwargs_to_map, map_to_py_dict, signature_types};
use crate::core::OwnerCell;

/// Plugin is a class that represents a loaded plugin and its namespace.
#[pyclass(name = "Plugin", frozen, from_py_object)]
#[derive(Clone)]
pub(crate) struct PyPlugin {
  pub(crate) owner: Arc<OwnerCell>,
  /// The namespace of the plugin.
  pub(crate) namespace: CString,
}

#[pymethods]
impl PyPlugin {
  /// The namespace of the plugin.
  #[getter]
  fn namespace(&self) -> String {
    self.namespace.to_string_lossy().into_owned()
  }

  /// The name string of the plugin.
  #[getter]
  fn name(&self) -> String {
    self.owner.with_core(|core| {
      let plugin = core
        .get_plugin_by_namespace(&self.namespace)
        .expect("validated at construction");
      plugin.name().to_string_lossy().into_owned()
    })
  }

  /// The plugin identifier string.
  #[getter]
  fn identifier(&self) -> String {
    self.owner.with_core(|core| {
      let plugin = core
        .get_plugin_by_namespace(&self.namespace)
        .expect("validated at construction");
      plugin.id().to_string_lossy().into_owned()
    })
  }

  /// The version of the plugin returned as a `PluginVersion` (major, minor)
  /// tuple.
  #[getter]
  fn version<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
    let ver = self.owner.with_core(|core| {
      let plugin = core
        .get_plugin_by_namespace(&self.namespace)
        .expect("validated at construction");
      plugin.version()
    });
    let major = ver >> 16;
    let minor = if major > -1 { ver - (major << 16) } else { 0 };
    PyTuple::new(py, [major, minor])
  }

  /// The main library location of the plugin. Returns None for internal
  /// functions.
  #[getter]
  fn plugin_path(&self) -> Option<String> {
    self.owner.with_core(|core| {
      let plugin = core
        .get_plugin_by_namespace(&self.namespace)
        .expect("validated at construction");
      let path = plugin.path();
      let s = path.to_string_lossy();
      if s.is_empty() {
        None
      } else {
        Some(s.into_owned())
      }
    })
  }

  /// Yields all functions in the plugin as an iterator of `Function` objects.
  fn functions(&self) -> PyFunctionIter {
    let names: Vec<CString> = self.owner.with_core(|core| {
      let plugin = core
        .get_plugin_by_namespace(&self.namespace)
        .expect("validated at construction");
      plugin.functions().map(|f| f.name().into()).collect()
    });
    PyFunctionIter {
      owner: self.owner.clone(),
      namespace: self.namespace.clone(),
      names,
      index: 0,
    }
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
      plugin: self.clone(),
    })
  }

  fn __repr__(&self) -> String {
    format!(
      "<rynth.Plugin namespace={} name={}>",
      self.namespace.to_string_lossy(),
      self.owner.with_core(|core| {
        let plugin = core
          .get_plugin_by_namespace(&self.namespace)
          .expect("validated at construction");
        plugin.name().to_string_lossy().into_owned()
      })
    )
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
  /// The Plugin object the function belongs to.
  plugin: PyPlugin,
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

  /// The function name. Identical to the string used to register the function.
  #[getter]
  fn name(&self) -> String {
    self.name.to_string_lossy().into_owned()
  }

  /// The Plugin object the function belongs to.
  #[getter]
  fn plugin(&self) -> PyPlugin {
    self.plugin.clone()
  }

  /// Raw function signature string. Identical to the string used to register
  /// the function.
  #[getter]
  fn signature(&self) -> String {
    self.owner.with_core(|core| {
      let plugin = core
        .get_plugin_by_namespace(&self.namespace)
        .expect("validated at construction");
      let func = plugin
        .get_function_by_name(&self.name)
        .expect("validated at construction");
      func.arguments().to_string_lossy().into_owned()
    })
  }

  /// Raw function return type signature string. Identical to the return type
  /// string used to register the function.
  #[getter]
  fn return_signature(&self) -> String {
    self.owner.with_core(|core| {
      let plugin = core
        .get_plugin_by_namespace(&self.namespace)
        .expect("validated at construction");
      let func = plugin
        .get_function_by_name(&self.name)
        .expect("validated at construction");
      func.return_type().to_string_lossy().into_owned()
    })
  }

  fn __repr__(&self) -> String {
    format!(
      "<rynth.Function {}.{}>",
      self.namespace.to_string_lossy(),
      self.name.to_string_lossy()
    )
  }
}

/// Iterator over the functions in a plugin.
#[pyclass(name = "PluginFunctionIter")]
pub(crate) struct PyFunctionIter {
  owner: Arc<OwnerCell>,
  namespace: CString,
  names: Vec<CString>,
  index: usize,
}

#[pymethods]
impl PyFunctionIter {
  #[allow(clippy::missing_const_for_fn)]
  fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
    slf
  }

  fn __next__(&mut self) -> Option<PyFunction> {
    let name = self.names.get(self.index)?.clone();
    self.index += 1;
    Some(PyFunction {
      owner: self.owner.clone(),
      namespace: self.namespace.clone(),
      name,
      plugin: PyPlugin {
        owner: self.owner.clone(),
        namespace: self.namespace.clone(),
      },
    })
  }
}

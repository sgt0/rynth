//! Python bindings for VapourSynth.

mod api;
mod convert;
mod core;
mod enums;
mod environment;
mod frame;
mod map_ext;
mod node;
mod plugin;
mod std_ns;

use pyo3::prelude::*;

/// Python bindings for VapourSynth.
#[pymodule]
mod rynth {
  use super::{Bound, Py, PyModule, PyModuleMethods, PyResult};

  #[pymodule_export]
  use crate::core::{PyCore, PyCoreProxy, PyPluginIter};
  #[pymodule_export]
  use crate::enums::{ColorFamily, SampleType};
  #[pymodule_export]
  use crate::environment::{
    Environment, EnvironmentPolicy, EnvironmentPolicyAPI, StandaloneEnvironmentPolicy,
    VideoOutputTuple,
  };
  #[pymodule_export]
  use crate::environment::{
    clear_output, clear_outputs, clear_policy, get_current_environment, get_output, get_outputs,
    has_policy, register_policy,
  };
  #[pymodule_export]
  use crate::frame::{PyVideoFormat, PyVideoFrame};
  #[pymodule_export]
  use crate::node::{PyFrameIter, PyVideoNode};
  #[pymodule_export]
  use crate::plugin::{PyFunction, PyFunctionIter, PyPlugin};

  #[pymodule_init]
  fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("core", Py::new(m.py(), crate::core::PyCoreProxy)?)?;
    Ok(())
  }
}

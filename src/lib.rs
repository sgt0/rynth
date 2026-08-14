//! Python bindings for VapourSynth.

mod api;
mod convert;
mod core;
mod enums;
mod frame;
mod node;
mod plugin;

use pyo3::prelude::*;

/// Python bindings for VapourSynth.
#[pymodule]
mod rynth {
  use super::{Bound, Py, PyModule, PyModuleMethods, PyResult};

  #[pymodule_export]
  use crate::core::{PyCore, PyCoreProxy};
  #[pymodule_export]
  use crate::enums::{ColorFamily, SampleType};
  #[pymodule_export]
  use crate::frame::{PyVideoFormat, PyVideoFrame};
  #[pymodule_export]
  use crate::node::{PyFrameIter, PyVideoNode};
  #[pymodule_export]
  use crate::plugin::{PyFunction, PyPlugin};

  #[pymodule_init]
  fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("core", Py::new(m.py(), crate::core::PyCoreProxy)?)?;
    Ok(())
  }
}

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
  use crate::enums::{ColorFamily, PresetVideoFormat, SampleType};
  #[pymodule_export]
  use crate::environment::{
    Environment, EnvironmentData, EnvironmentPolicy, EnvironmentPolicyAPI,
    StandaloneEnvironmentPolicy, VideoOutputTuple,
  };
  #[pymodule_export]
  use crate::environment::{
    clear_output, clear_outputs, clear_policy, get_current_environment, get_output, get_outputs,
    has_policy, register_policy,
  };
  #[pymodule_export]
  use crate::frame::{PyAudioFrame, PyRawFrame, PyVideoFormat, PyVideoFrame};
  #[pymodule_export]
  use crate::node::{PyAudioFrameIter, PyAudioNode, PyVideoFrameIter, PyVideoNode};
  #[pymodule_export]
  use crate::plugin::{PyFunction, PyFunctionIter, PyPlugin};

  #[pymodule_export]
  const NONE: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::None;
  #[pymodule_export]
  const GRAY8: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::Gray8;
  #[pymodule_export]
  const GRAY9: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::Gray9;
  #[pymodule_export]
  const GRAY10: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::Gray10;
  #[pymodule_export]
  const GRAY12: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::Gray12;
  #[pymodule_export]
  const GRAY14: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::Gray14;
  #[pymodule_export]
  const GRAY16: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::Gray16;
  #[pymodule_export]
  const GRAY32: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::Gray32;
  #[pymodule_export]
  const GRAYH: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::GrayH;
  #[pymodule_export]
  const GRAYS: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::GrayS;
  #[pymodule_export]
  const YUV410P8: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV410P8;
  #[pymodule_export]
  const YUV411P8: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV411P8;
  #[pymodule_export]
  const YUV440P8: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV440P8;
  #[pymodule_export]
  const YUV420P8: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV420P8;
  #[pymodule_export]
  const YUV422P8: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV422P8;
  #[pymodule_export]
  const YUV444P8: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV444P8;
  #[pymodule_export]
  const YUV420P9: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV420P9;
  #[pymodule_export]
  const YUV422P9: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV422P9;
  #[pymodule_export]
  const YUV444P9: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV444P9;
  #[pymodule_export]
  const YUV420P10: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV420P10;
  #[pymodule_export]
  const YUV422P10: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV422P10;
  #[pymodule_export]
  const YUV444P10: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV444P10;
  #[pymodule_export]
  const YUV420P12: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV420P12;
  #[pymodule_export]
  const YUV422P12: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV422P12;
  #[pymodule_export]
  const YUV444P12: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV444P12;
  #[pymodule_export]
  const YUV420P14: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV420P14;
  #[pymodule_export]
  const YUV422P14: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV422P14;
  #[pymodule_export]
  const YUV444P14: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV444P14;
  #[pymodule_export]
  const YUV410P16: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV410P16;
  #[pymodule_export]
  const YUV411P16: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV411P16;
  #[pymodule_export]
  const YUV440P16: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV440P16;
  #[pymodule_export]
  const YUV420P16: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV420P16;
  #[pymodule_export]
  const YUV422P16: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV422P16;
  #[pymodule_export]
  const YUV444P16: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV444P16;
  #[pymodule_export]
  const YUV410PH: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV410PH;
  #[pymodule_export]
  const YUV410PS: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV410PS;
  #[pymodule_export]
  const YUV411PH: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV411PH;
  #[pymodule_export]
  const YUV411PS: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV411PS;
  #[pymodule_export]
  const YUV440PH: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV440PH;
  #[pymodule_export]
  const YUV440PS: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV440PS;
  #[pymodule_export]
  const YUV420PH: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV420PH;
  #[pymodule_export]
  const YUV420PS: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV420PS;
  #[pymodule_export]
  const YUV422PH: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV422PH;
  #[pymodule_export]
  const YUV422PS: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV422PS;
  #[pymodule_export]
  const YUV444PH: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV444PH;
  #[pymodule_export]
  const YUV444PS: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::YUV444PS;
  #[pymodule_export]
  const RGB24: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::RGB24;
  #[pymodule_export]
  const RGB27: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::RGB27;
  #[pymodule_export]
  const RGB30: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::RGB30;
  #[pymodule_export]
  const RGB36: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::RGB36;
  #[pymodule_export]
  const RGB42: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::RGB42;
  #[pymodule_export]
  const RGB48: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::RGB48;
  #[pymodule_export]
  const RGBH: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::RgbH;
  #[pymodule_export]
  const RGBS: crate::enums::PresetVideoFormat = crate::enums::PresetVideoFormat::RgbS;

  #[pymodule_init]
  fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("core", Py::new(m.py(), crate::core::PyCoreProxy)?)?;
    Ok(())
  }
}

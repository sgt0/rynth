//! VapourSynth format enums exposed to Python.

use pyo3::prelude::*;
use vapoursynth4_rs::{ColorFamily as VsColorFamily, SampleType as VsSampleType};

/// The color family of a video format.
#[pyclass(
  eq,
  eq_int,
  frozen,
  hash,
  skip_from_py_object,
  name = "ColorFamily",
  module = "rynth"
)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ColorFamily {
  /// UNDEFINED.
  #[pyo3(name = "UNDEFINED")]
  Undefined = 0,
  // GRAY.
  #[pyo3(name = "GRAY")]
  Gray = 1,
  /// RGB.
  #[pyo3(name = "RGB")]
  Rgb = 2,
  /// YUV.
  #[pyo3(name = "YUV")]
  Yuv = 3,
}

impl From<VsColorFamily> for ColorFamily {
  fn from(value: VsColorFamily) -> Self {
    match value {
      VsColorFamily::Undefined => Self::Undefined,
      VsColorFamily::Gray => Self::Gray,
      VsColorFamily::RGB => Self::Rgb,
      VsColorFamily::YUV => Self::Yuv,
    }
  }
}

/// The sample type of a video format.
#[pyclass(
  eq,
  eq_int,
  frozen,
  hash,
  skip_from_py_object,
  name = "SampleType",
  module = "rynth"
)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SampleType {
  /// Integer.
  #[pyo3(name = "INTEGER")]
  Integer = 0,
  /// Float.
  #[pyo3(name = "FLOAT")]
  Float = 1,
}

impl From<VsSampleType> for SampleType {
  fn from(value: VsSampleType) -> Self {
    match value {
      VsSampleType::Integer => Self::Integer,
      VsSampleType::Float => Self::Float,
    }
  }
}

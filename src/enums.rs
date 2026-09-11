//! VapourSynth format enums exposed to Python.

use pyo3::prelude::*;
use vapoursynth4_rs::ffi::VSPresetVideoFormat as Preset;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

/// VapourSynth video format presets.
#[pyclass(
  eq,
  eq_int,
  frozen,
  hash,
  from_py_object,
  name = "PresetVideoFormat",
  module = "rynth"
)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PresetVideoFormat {
  /// `NONE`.
  #[pyo3(name = "NONE")]
  None = Preset::None as isize,
  /// `GRAY8`.
  #[pyo3(name = "GRAY8")]
  Gray8 = Preset::Gray8 as isize,
  /// `GRAY9`.
  #[pyo3(name = "GRAY9")]
  Gray9 = Preset::Gray9 as isize,
  /// `GRAY10`.
  #[pyo3(name = "GRAY10")]
  Gray10 = Preset::Gray10 as isize,
  /// `GRAY12`.
  #[pyo3(name = "GRAY12")]
  Gray12 = Preset::Gray12 as isize,
  /// `GRAY14`.
  #[pyo3(name = "GRAY14")]
  Gray14 = Preset::Gray14 as isize,
  /// `GRAY16`.
  #[pyo3(name = "GRAY16")]
  Gray16 = Preset::Gray16 as isize,
  /// `GRAY32`.
  #[pyo3(name = "GRAY32")]
  Gray32 = Preset::Gray32 as isize,
  /// `GRAYH`.
  #[pyo3(name = "GRAYH")]
  GrayH = Preset::GrayH as isize,
  /// `GRAYS`.
  #[pyo3(name = "GRAYS")]
  GrayS = Preset::GrayS as isize,
  /// `YUV410P8`.
  YUV410P8 = Preset::YUV410P8 as isize,
  /// `YUV411P8`.
  YUV411P8 = Preset::YUV411P8 as isize,
  /// `YUV440P8`.
  YUV440P8 = Preset::YUV440P8 as isize,
  /// `YUV420P8`.
  YUV420P8 = Preset::YUV420P8 as isize,
  /// `YUV422P8`.
  YUV422P8 = Preset::YUV422P8 as isize,
  /// `YUV444P8`.
  YUV444P8 = Preset::YUV444P8 as isize,
  /// `YUV420P9`.
  YUV420P9 = Preset::YUV420P9 as isize,
  /// `YUV422P9`.
  YUV422P9 = Preset::YUV422P9 as isize,
  /// `YUV444P9`.
  YUV444P9 = Preset::YUV444P9 as isize,
  /// `YUV420P10`.
  YUV420P10 = Preset::YUV420P10 as isize,
  /// `YUV422P10`.
  YUV422P10 = Preset::YUV422P10 as isize,
  /// `YUV444P10`.
  YUV444P10 = Preset::YUV444P10 as isize,
  /// `YUV420P12`.
  YUV420P12 = Preset::YUV420P12 as isize,
  /// `YUV422P12`.
  YUV422P12 = Preset::YUV422P12 as isize,
  /// `YUV444P12`.
  YUV444P12 = Preset::YUV444P12 as isize,
  /// `YUV420P14`.
  YUV420P14 = Preset::YUV420P14 as isize,
  /// `YUV422P14`.
  YUV422P14 = Preset::YUV422P14 as isize,
  /// `YUV444P14`.
  YUV444P14 = Preset::YUV444P14 as isize,
  /// `YUV410P16`.
  YUV410P16 = Preset::YUV410P16 as isize,
  /// `YUV411P16`.
  YUV411P16 = Preset::YUV411P16 as isize,
  /// `YUV440P16`.
  YUV440P16 = Preset::YUV440P16 as isize,
  /// `YUV420P16`.
  YUV420P16 = Preset::YUV420P16 as isize,
  /// `YUV422P16`.
  YUV422P16 = Preset::YUV422P16 as isize,
  /// `YUV444P16`.
  YUV444P16 = Preset::YUV444P16 as isize,
  /// `YUV410PH`.
  YUV410PH = Preset::YUV410PH as isize,
  /// `YUV410PS`.
  YUV410PS = Preset::YUV410PS as isize,
  /// `YUV411PH`.
  YUV411PH = Preset::YUV411PH as isize,
  /// `YUV411PS`.
  YUV411PS = Preset::YUV411PS as isize,
  /// `YUV440PH`.
  YUV440PH = Preset::YUV440PH as isize,
  /// `YUV440PS`.
  YUV440PS = Preset::YUV440PS as isize,
  /// `YUV420PH`.
  YUV420PH = Preset::YUV420PH as isize,
  /// `YUV420PS`.
  YUV420PS = Preset::YUV420PS as isize,
  /// `YUV422PH`.
  YUV422PH = Preset::YUV422PH as isize,
  /// `YUV422PS`.
  YUV422PS = Preset::YUV422PS as isize,
  /// `YUV444PH`.
  YUV444PH = Preset::YUV444PH as isize,
  /// `YUV444PS`.
  YUV444PS = Preset::YUV444PS as isize,
  /// `RGB24`.
  RGB24 = Preset::RGB24 as isize,
  /// `RGB27`.
  RGB27 = Preset::RGB27 as isize,
  /// `RGB30`.
  RGB30 = Preset::RGB30 as isize,
  /// `RGB36`.
  RGB36 = Preset::RGB36 as isize,
  /// `RGB42`.
  RGB42 = Preset::RGB42 as isize,
  /// `RGB48`.
  RGB48 = Preset::RGB48 as isize,
  /// `RGBH`.
  #[pyo3(name = "RGBH")]
  RgbH = Preset::RGBH as isize,
  /// `RGBS`.
  #[pyo3(name = "RGBS")]
  RgbS = Preset::RGBS as isize,
}

//! Video frames and zero-copy plane access.

use std::ffi::{CString, c_int};
use std::sync::{Arc, OnceLock};

use pyo3::exceptions::{PyBufferError, PyIndexError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyMemoryView};
use vapoursynth4_rs::ffi;
use vapoursynth4_rs::frame::{Frame, VideoFormat, VideoFrame};

use crate::convert::map_to_py_dict;
use crate::core::OwnerCell;
use crate::enums::{ColorFamily, SampleType};

/// Read-only VS frame shared across planes/memoryviews.
pub(crate) struct FrameCell(pub(crate) VideoFrame);

// SAFETY: VSFrame contents are immutable once returned by getFrame. Concurrent
// reads are safe and dropping happens once via Arc.
unsafe impl Send for FrameCell {}
// SAFETY: same as `Send` above. The frame is never mutated.
unsafe impl Sync for FrameCell {}

/// Represents a video frame and all metadata attached to it.
#[pyclass(name = "VideoFrame", module = "rynth", frozen)]
pub(crate) struct PyVideoFrame {
  pub(crate) frame: Arc<FrameCell>,
  pub(crate) owner: Arc<OwnerCell>,
  format: OnceLock<Py<PyVideoFormat>>,
}

impl PyVideoFrame {
  pub(crate) fn new(frame: VideoFrame, owner: Arc<OwnerCell>) -> Self {
    Self {
      frame: Arc::new(FrameCell(frame)),
      owner,
      format: OnceLock::new(),
    }
  }
}

#[pymethods]
impl PyVideoFrame {
  /// The width of the frame.
  #[getter]
  fn width(&self) -> i32 {
    self.frame.0.frame_width(0)
  }

  /// The height of the frame.
  #[getter]
  fn height(&self) -> i32 {
    self.frame.0.frame_height(0)
  }

  /// This attribute holds all the frame's properties as a dict.
  #[getter]
  fn props(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
    self.frame.0.properties().map_or_else(
      || Ok(PyDict::new(py).unbind()),
      |map| map_to_py_dict(py, &map, &self.owner),
    )
  }

  /// The frame's video format. Built once from the core and shared thereafter.
  #[getter]
  fn format(&self, py: Python<'_>) -> PyResult<Py<PyVideoFormat>> {
    if let Some(format) = self.format.get() {
      return Ok(format.clone_ref(py));
    }
    let format = Py::new(
      py,
      PyVideoFormat::from_vs(self.frame.0.get_video_format(), &self.owner),
    )?;
    Ok(self.format.get_or_init(|| format).clone_ref(py))
  }

  /// Zero-copy plane accessor. `frame[plane_idx]` returns a read-only
  /// `memoryview` over the whole plane, matching the VapourSynth Python API.
  fn __getitem__(&self, py: Python<'_>, index: i32) -> PyResult<Py<PyMemoryView>> {
    let frame = &self.frame.0;
    let format = frame.get_video_format();
    if index < 0 || index >= format.num_planes {
      return Err(PyIndexError::new_err("index out of range"));
    }
    let (fmt, itemsize) = match (format.sample_type, format.bytes_per_sample) {
      (ffi::VSSampleType::Integer, 1) => (c"B", 1),
      (ffi::VSSampleType::Integer, 2) => (c"H", 2),
      (ffi::VSSampleType::Integer, 4) => (c"I", 4),
      (ffi::VSSampleType::Float, 2) => (c"e", 2),
      (ffi::VSSampleType::Float, 4) => (c"f", 4),
      (st, bps) => {
        return Err(PyRuntimeError::new_err(format!(
          "unsupported sample layout: {st:?}/{bps} bytes"
        )));
      }
    };
    let plane = Bound::new(
      py,
      PyPlane {
        _frame: self.frame.clone(),
        data: frame.plane(index),
        shape: [
          frame.frame_height(index) as isize,
          frame.frame_width(index) as isize,
        ],
        strides: [frame.stride(index), itemsize],
        itemsize,
        format: fmt.into(),
      },
    )?;
    Ok(PyMemoryView::from(plane.as_any())?.unbind())
  }

  /// The number of planes, so the frame acts as a sequence of planes.
  fn __len__(&self) -> usize {
    self.frame.0.get_video_format().num_planes as usize
  }

  fn __repr__(slf: &Bound<'_, Self>) -> String {
    let this = slf.get();
    let address = slf.as_ptr() as usize;
    let format = this.format(slf.py()).map_or_else(
      |_| "dynamic".to_owned(),
      |f| f.bind(slf.py()).get().name.clone(),
    );
    let (width, height) = match (this.width(), this.height()) {
      (w, h) if w != 0 && h != 0 => (w.to_string(), h.to_string()),
      _ => ("dynamic".to_owned(), "dynamic".to_owned()),
    };
    format!(
      "<rynth.VideoFrame object at 0x{address:016X} \
       format={format}, width={width}, height={height}, readonly=True>"
    )
  }
}

/// Describes the format of a clip. Mirrors VapourSynth's `VideoFormat`.
#[pyclass(name = "VideoFormat", module = "rynth", frozen)]
pub(crate) struct PyVideoFormat {
  #[pyo3(get)]
  id: u32,
  #[pyo3(get)]
  name: String,
  #[pyo3(get)]
  color_family: ColorFamily,
  #[pyo3(get)]
  sample_type: SampleType,
  #[pyo3(get)]
  bits_per_sample: i32,
  #[pyo3(get)]
  bytes_per_sample: i32,
  #[pyo3(get)]
  subsampling_w: i32,
  #[pyo3(get)]
  subsampling_h: i32,
  #[pyo3(get)]
  num_planes: i32,
}

impl PyVideoFormat {
  /// Builds the format from a raw `VSVideoFormat`, resolving the name and id
  /// through the core exactly once, like Cython's `createVideoFormat`.
  fn from_vs(format: &VideoFormat, owner: &OwnerCell) -> Self {
    let name = owner
      .with_core(|core| core.get_video_format_name(format))
      .unwrap_or_else(|| "None".to_owned());
    let id = owner.with_core(|core| {
      core.query_video_format_id(
        format.color_family,
        format.sample_type,
        format.bits_per_sample,
        format.sub_sampling_w,
        format.sub_sampling_h,
      )
    });
    Self {
      id,
      name,
      color_family: format.color_family.into(),
      sample_type: format.sample_type.into(),
      bits_per_sample: format.bits_per_sample,
      bytes_per_sample: format.bytes_per_sample,
      subsampling_w: format.sub_sampling_w,
      subsampling_h: format.sub_sampling_h,
      num_planes: format.num_planes,
    }
  }
}

#[pyclass(name = "Plane", frozen)]
pub(crate) struct PyPlane {
  /// Guard to keep the frame alive while views exist.
  _frame: Arc<FrameCell>,
  data: *const u8,
  shape: [isize; 2],
  strides: [isize; 2],
  itemsize: isize,
  format: CString,
}

// SAFETY: `data` points into the immutable frame kept alive by `_frame`.
unsafe impl Send for PyPlane {}
// SAFETY: same as `Send` above; plane data is read-only.
unsafe impl Sync for PyPlane {}

#[pymethods]
impl PyPlane {
  #[allow(clippy::needless_pass_by_value)] // buffer protocol signature fixed by pyo3
  unsafe fn __getbuffer__(
    slf: PyRef<'_, Self>,
    view: *mut pyo3::ffi::Py_buffer,
    flags: c_int,
  ) -> PyResult<()> {
    if view.is_null() {
      return Err(PyBufferError::new_err("null view"));
    }
    if flags & pyo3::ffi::PyBUF_WRITABLE != 0 {
      return Err(PyBufferError::new_err("frame planes are read-only"));
    }
    if flags & pyo3::ffi::PyBUF_STRIDES != pyo3::ffi::PyBUF_STRIDES {
      return Err(PyBufferError::new_err(
        "strided buffer required (planes are not C-contiguous)",
      ));
    }
    // SAFETY: `view` was checked non-null above and CPython hands us an
    // exclusive, valid Py_buffer to fill.
    let v = unsafe { &mut *view };
    v.buf = slf.data.cast_mut().cast();
    v.obj = slf.as_ptr();
    // SAFETY: `v.obj` is a valid object pointer; the buffer holds one strong
    // reference to it until __releasebuffer__.
    unsafe { pyo3::ffi::Py_INCREF(v.obj) };
    v.len = slf.shape[0] * slf.shape[1] * slf.itemsize;
    v.readonly = 1;
    v.itemsize = slf.itemsize;
    v.format = if flags & pyo3::ffi::PyBUF_FORMAT == pyo3::ffi::PyBUF_FORMAT {
      slf.format.as_ptr().cast_mut()
    } else {
      std::ptr::null_mut()
    };
    v.ndim = 2;
    v.shape = slf.shape.as_ptr().cast_mut();
    v.strides = slf.strides.as_ptr().cast_mut();
    v.suboffsets = std::ptr::null_mut();
    v.internal = std::ptr::null_mut();
    Ok(())
  }

  #[allow(clippy::unused_self)] // buffer protocol method must be an instance method
  const unsafe fn __releasebuffer__(&self, _view: *mut pyo3::ffi::Py_buffer) {
    // This is a no-op because we defer to CPython to drop the Py_buffer's
    // `obj` reference.
  }
}

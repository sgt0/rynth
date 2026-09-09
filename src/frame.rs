//! Video frames and zero-copy plane access.

use std::ffi::{CString, c_int};
use std::sync::{Arc, OnceLock};

use pyo3::exceptions::{PyBufferError, PyIndexError, PyNotImplementedError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyMemoryView};
use vapoursynth4_rs::ffi;
use vapoursynth4_rs::frame::{AudioFrame, Frame, VideoFormat, VideoFrame};

use crate::convert::map_to_py_dict;
use crate::core::OwnerCell;
use crate::enums::{ColorFamily, SampleType};

/// A read-only VS frame (video or audio) shared across planes/memoryviews.
pub(crate) enum FrameCell {
  Video(VideoFrame),
  Audio(AudioFrame),
}

// SAFETY: VSFrame contents are immutable once returned by getFrame. Concurrent
// reads are safe and dropping happens once via Arc.
unsafe impl Send for FrameCell {}
// SAFETY: same as `Send` above. The frame is never mutated.
unsafe impl Sync for FrameCell {}

impl FrameCell {
  /// Number of addressable data buffers: planes for video, channels for audio.
  fn num_data(&self) -> i32 {
    match self {
      Self::Video(f) => f.get_video_format().num_planes,
      Self::Audio(f) => f.get_audio_format().num_channels,
    }
  }

  /// Read pointer for the given plane/channel.
  fn read_ptr(&self, index: i32) -> *const u8 {
    match self {
      Self::Video(f) => f.plane(index),
      Self::Audio(f) => f.channel(index),
    }
  }

  /// Write pointer for the given plane/channel.
  fn write_ptr(&self, index: i32) -> *mut u8 {
    // SAFETY: writable frames come from VapourSynth, who owns the allocation
    // and keeps it valid for the frame's lifetime.
    unsafe {
      match self {
        Self::Video(f) => (f.api().getWritePtr)(f.as_ptr(), index),
        Self::Audio(f) => (f.api().getWritePtr)(f.as_ptr(), index),
      }
    }
  }

  /// Stride between rows of a plane.
  fn stride(&self, index: i32) -> isize {
    match self {
      Self::Video(f) => f.stride(index),
      Self::Audio(_) => 0, // Only video frames are strided.
    }
  }

  pub(crate) fn video(&self) -> &VideoFrame {
    match self {
      Self::Video(f) => f,
      Self::Audio(_) => unreachable!("video frame expected"),
    }
  }

  pub(crate) fn audio(&self) -> &AudioFrame {
    match self {
      Self::Audio(f) => f,
      Self::Video(_) => unreachable!("audio frame expected"),
    }
  }
}

/// Common lifecycle and data access for audio and video frames.
#[pyclass(name = "RawFrame", module = "rynth", frozen, subclass, weakref)]
pub(crate) struct PyRawFrame {
  frame: parking_lot::Mutex<Option<Arc<FrameCell>>>,
  pub(crate) owner: Arc<OwnerCell>,
  readonly: bool,
}

impl PyRawFrame {
  fn new(frame: FrameCell, owner: Arc<OwnerCell>, readonly: bool) -> Self {
    Self {
      frame: parking_lot::Mutex::new(Some(Arc::new(frame))),
      owner,
      readonly,
    }
  }

  fn frame(&self) -> PyResult<Arc<FrameCell>> {
    self
      .frame
      .lock()
      .clone()
      .ok_or_else(|| PyRuntimeError::new_err("The Frame has already been released."))
  }
}

#[pymethods]
impl PyRawFrame {
  /// Whether or not the frame has been closed.
  #[getter]
  fn closed(&self) -> bool {
    self.frame.lock().is_none()
  }

  /// Whether or not the frame data and properties cannot be modified.
  #[getter]
  const fn readonly(&self) -> bool {
    self.readonly
  }

  /// Forcefully releases this frame.
  fn close(&self) {
    self.frame.lock().take();
  }

  const fn __enter__(slf: Py<Self>) -> Py<Self> {
    slf
  }

  fn __exit__(
    &self,
    _exc_type: Option<&Bound<'_, PyAny>>,
    _exc_value: Option<&Bound<'_, PyAny>>,
    _traceback: Option<&Bound<'_, PyAny>>,
  ) {
    self.close();
  }

  /// This frame's properties.
  #[getter]
  fn props(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
    let frame = self.frame()?;
    let map = match &*frame {
      FrameCell::Video(f) => f.properties(),
      FrameCell::Audio(f) => f.properties(),
    };
    map.map_or_else(
      || Ok(PyDict::new(py).unbind()),
      |map| map_to_py_dict(py, &map, &self.owner),
    )
  }

  /// Returns a pointer to the raw frame data. The data may not be modified.
  fn get_read_ptr(&self, plane: i32) -> PyResult<usize> {
    let frame = self.frame()?;
    if plane < 0 || plane >= frame.num_data() {
      return Err(PyIndexError::new_err("Specified plane index out of range"));
    }
    Ok(frame.read_ptr(plane) as usize)
  }

  /// Returns a pointer to the raw frame data. It may be modified.
  fn get_write_ptr(&self, plane: i32) -> PyResult<usize> {
    if self.readonly {
      return Err(PyRuntimeError::new_err(
        "Can only obtain write pointer for writable frames",
      ));
    }
    let frame = self.frame()?;
    if plane < 0 || plane >= frame.num_data() {
      return Err(PyIndexError::new_err("Specified plane index out of range"));
    }
    Ok(frame.write_ptr(plane) as usize)
  }

  /// Returns the stride between lines in a plane.
  fn get_stride(&self, plane: i32) -> PyResult<isize> {
    let frame = self.frame()?;
    if plane < 0 || plane >= frame.num_data() {
      return Err(PyIndexError::new_err("Specified plane index out of range"));
    }
    Ok(frame.stride(plane))
  }

  /// Returns a writable copy of the frame.
  #[allow(clippy::unused_self)]
  fn copy(&self, _py: Python<'_>) -> PyResult<Py<PyAny>> {
    Err(PyNotImplementedError::new_err(()))
  }

  #[allow(clippy::unused_self)]
  fn __getitem__(&self, _index: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    Err(PyNotImplementedError::new_err(()))
  }

  #[allow(clippy::unused_self)]
  fn __len__(&self) -> PyResult<usize> {
    Err(PyNotImplementedError::new_err(()))
  }
}

/// Represents a video frame and all metadata attached to it.
#[pyclass(name = "VideoFrame", module = "rynth", frozen, extends = PyRawFrame)]
pub(crate) struct PyVideoFrame {
  format: OnceLock<Py<PyVideoFormat>>,
}

impl PyVideoFrame {
  pub(crate) fn new(frame: VideoFrame, owner: Arc<OwnerCell>) -> PyClassInitializer<Self> {
    PyClassInitializer::from(PyRawFrame::new(FrameCell::Video(frame), owner, true)).add_subclass(
      Self {
        format: OnceLock::new(),
      },
    )
  }

  fn frame(slf: &PyRef<'_, Self>) -> PyResult<Arc<FrameCell>> {
    slf.as_super().frame()
  }

  pub(crate) fn create(
    py: Python<'_>,
    frame: VideoFrame,
    owner: Arc<OwnerCell>,
  ) -> PyResult<Py<Self>> {
    Py::new(py, Self::new(frame, owner))
  }
}

#[pymethods]
impl PyVideoFrame {
  /// The width of the frame.
  #[getter]
  #[allow(clippy::needless_pass_by_value)]
  fn width(slf: PyRef<'_, Self>) -> PyResult<i32> {
    Ok(Self::frame(&slf)?.video().frame_width(0))
  }

  /// The height of the frame.
  #[getter]
  #[allow(clippy::needless_pass_by_value)]
  fn height(slf: PyRef<'_, Self>) -> PyResult<i32> {
    Ok(Self::frame(&slf)?.video().frame_height(0))
  }

  /// The frame's video format. Built once from the core and shared thereafter.
  #[getter]
  #[allow(clippy::needless_pass_by_value)]
  fn format(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyVideoFormat>> {
    if let Some(format) = slf.format.get() {
      return Ok(format.clone_ref(py));
    }
    let frame = Self::frame(&slf)?;
    let format = Py::new(
      py,
      PyVideoFormat::from_vs(frame.video().get_video_format(), &slf.as_super().owner),
    )?;
    Ok(slf.format.get_or_init(|| format).clone_ref(py))
  }

  /// Zero-copy plane accessor. `frame[plane_idx]` returns a read-only
  /// `memoryview` over the whole plane.
  #[allow(clippy::needless_pass_by_value)]
  fn __getitem__(
    slf: PyRef<'_, Self>,
    py: Python<'_>,
    mut index: i32,
  ) -> PyResult<Py<PyMemoryView>> {
    let frame = Self::frame(&slf)?;
    let format = frame.video().get_video_format();
    if index < 0 {
      index += format.num_planes;
    }
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
        _frame: frame.clone(),
        data: frame.video().plane(index),
        shape: [
          frame.video().frame_height(index) as isize,
          frame.video().frame_width(index) as isize,
        ],
        strides: [frame.video().stride(index), itemsize],
        ndim: 2,
        itemsize,
        format: fmt.into(),
      },
    )?;
    Ok(PyMemoryView::from(plane.as_any())?.unbind())
  }

  /// The number of planes, so the frame acts as a sequence of planes.
  #[allow(clippy::needless_pass_by_value)]
  fn __len__(slf: PyRef<'_, Self>) -> PyResult<usize> {
    Ok(Self::frame(&slf)?.video().get_video_format().num_planes as usize)
  }

  /// Returns a writable copy of the frame.
  #[allow(clippy::needless_pass_by_value)]
  fn copy(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<Self>> {
    let frame = Self::frame(&slf)?;
    let owner = slf.as_super().owner.clone();
    let copy = owner.with_core(|core| core.copy_frame(frame.video()));
    Py::new(
      py,
      PyClassInitializer::from(PyRawFrame::new(FrameCell::Video(copy), owner, false)).add_subclass(
        Self {
          format: OnceLock::new(),
        },
      ),
    )
  }

  #[allow(clippy::needless_pass_by_value)]
  fn __repr__(slf: PyRef<'_, Self>) -> String {
    let address = slf.as_ptr() as usize;
    let readonly = slf.as_super().readonly;
    let Ok(frame) = Self::frame(&slf) else {
      return format!("<rynth.VideoFrame object at 0x{address:016X} closed=True>");
    };
    let format = slf.format.get().map_or_else(
      || "dynamic".to_owned(),
      |f| f.bind(slf.py()).get().name.clone(),
    );
    let (width, height) = match (frame.video().frame_width(0), frame.video().frame_height(0)) {
      (w, h) if w != 0 && h != 0 => (w.to_string(), h.to_string()),
      _ => ("dynamic".to_owned(), "dynamic".to_owned()),
    };
    format!(
      "<rynth.VideoFrame object at 0x{address:016X} \
       format={format}, width={width}, height={height}, readonly={readonly}>"
    )
  }
}

/// Represents an audio frame and all metadata attached to it.
#[pyclass(name = "AudioFrame", module = "rynth", frozen, extends = PyRawFrame)]
pub(crate) struct PyAudioFrame {
  /// Whether the samples are integers or floating point.
  #[pyo3(get)]
  sample_type: SampleType,
  /// Number of significant bits per sample.
  #[pyo3(get)]
  bits_per_sample: i32,
  /// Number of bytes needed to store one sample.
  #[pyo3(get)]
  bytes_per_sample: i32,
  /// Bitmask of the channels present in the frame.
  #[pyo3(get)]
  channel_layout: u64,
  /// Number of audio channels.
  #[pyo3(get)]
  num_channels: i32,
}

impl PyAudioFrame {
  fn new(frame: AudioFrame, owner: Arc<OwnerCell>, readonly: bool) -> PyClassInitializer<Self> {
    let format = frame.get_audio_format();
    let meta = Self {
      sample_type: format.sample_type.into(),
      bits_per_sample: format.bits_per_sample,
      bytes_per_sample: format.bytes_per_sample,
      channel_layout: format.channel_layout,
      num_channels: format.num_channels,
    };
    PyClassInitializer::from(PyRawFrame::new(FrameCell::Audio(frame), owner, readonly))
      .add_subclass(meta)
  }

  fn frame(slf: &PyRef<'_, Self>) -> PyResult<Arc<FrameCell>> {
    slf.as_super().frame()
  }

  pub(crate) fn create(
    py: Python<'_>,
    frame: AudioFrame,
    owner: Arc<OwnerCell>,
  ) -> PyResult<Py<Self>> {
    Py::new(py, Self::new(frame, owner, true))
  }
}

#[pymethods]
impl PyAudioFrame {
  /// Zero-copy channel accessor. `frame[channel]` returns a read-only,
  /// one-dimensional `memoryview` over the channel's samples.
  #[allow(clippy::needless_pass_by_value)]
  fn __getitem__(
    slf: PyRef<'_, Self>,
    py: Python<'_>,
    mut index: i32,
  ) -> PyResult<Py<PyMemoryView>> {
    let frame = Self::frame(&slf)?;
    let audio = frame.audio();
    if index < 0 {
      index += slf.num_channels;
    }
    if index < 0 || index >= slf.num_channels {
      return Err(PyIndexError::new_err("index out of range"));
    }
    let (fmt, itemsize) = match (slf.sample_type, slf.bytes_per_sample) {
      (SampleType::Integer, 2) => (c"h", 2),
      (SampleType::Integer, 4) => (c"i", 4),
      (SampleType::Float, 2) => (c"e", 2),
      (SampleType::Float, 4) => (c"f", 4),
      (st, bps) => {
        return Err(PyRuntimeError::new_err(format!(
          "unsupported sample layout: {st:?}/{bps} bytes"
        )));
      }
    };
    let plane = Bound::new(
      py,
      PyPlane {
        _frame: frame.clone(),
        data: audio.channel(index),
        shape: [audio.frame_length() as isize, 0],
        strides: [itemsize, 0],
        ndim: 1,
        itemsize,
        format: fmt.into(),
      },
    )?;
    Ok(PyMemoryView::from(plane.as_any())?.unbind())
  }

  /// The number of channels.
  #[allow(clippy::needless_pass_by_value)]
  fn __len__(slf: PyRef<'_, Self>) -> PyResult<usize> {
    Self::frame(&slf)?;
    Ok(slf.num_channels as usize)
  }

  #[allow(clippy::needless_pass_by_value)]
  fn __repr__(slf: PyRef<'_, Self>) -> String {
    let address = slf.as_ptr() as usize;
    let readonly = slf.as_super().readonly;
    if Self::frame(&slf).is_err() {
      return format!("<rynth.AudioFrame object at 0x{address:016X} closed=True>");
    }
    format!(
      "<rynth.AudioFrame object at 0x{address:016X} \
       sample_type={:?}, bits_per_sample={}, num_channels={}, readonly={readonly}>",
      slf.sample_type, slf.bits_per_sample, slf.num_channels
    )
  }
}

/// Represents all information needed to describe a frame format. It holds the
/// general color type, subsampling, number of planes, and so on
#[pyclass(name = "VideoFormat", module = "rynth", frozen)]
pub(crate) struct PyVideoFormat {
  /// A unique id identifying the format.
  #[pyo3(get)]
  id: u32,
  /// A human readable name of the format.
  #[pyo3(get)]
  name: String,
  /// Which group of colorspaces the format describes.
  #[pyo3(get)]
  color_family: ColorFamily,
  /// If the format is integer or floating point based.
  #[pyo3(get)]
  sample_type: SampleType,
  /// How many bits are used to store one sample in one plane.
  #[pyo3(get)]
  bits_per_sample: i32,
  /// The actual storage is padded up to 2^n bytes for efficiency.
  #[pyo3(get)]
  bytes_per_sample: i32,
  /// The subsampling for the second and third plane in the horizontal direction.
  #[pyo3(get)]
  subsampling_w: i32,
  /// The subsampling for the second and third plane in the vertical direction.
  #[pyo3(get)]
  subsampling_h: i32,
  /// The number of planes the format has.
  #[pyo3(get)]
  num_planes: i32,
}

impl PyVideoFormat {
  /// Builds the format from a raw `VSVideoFormat`, resolving the name and id
  /// through the core exactly once, like Cython's `createVideoFormat`.
  fn from_vs(format: &VideoFormat, owner: &OwnerCell) -> Self {
    let name = owner
      .with_core(|core| core.get_video_format_name(format))
      .map_or_else(|| "None".to_owned(), trim_format_name);
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

pub(crate) fn trim_format_name(mut name: String) -> String {
  name.truncate(name.find('\0').unwrap_or(name.len()));
  name
}

#[pyclass(name = "Plane", frozen)]
pub(crate) struct PyPlane {
  /// Guard to keep the frame alive while views exist.
  _frame: Arc<FrameCell>,
  data: *const u8,
  shape: [isize; 2],
  strides: [isize; 2],
  /// Number of dimensions the buffer exposes: 2 for video planes, 1 for audio
  /// channels.
  ndim: c_int,
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
    v.len = slf.itemsize * slf.shape[..slf.ndim as usize].iter().product::<isize>();
    v.readonly = 1;
    v.itemsize = slf.itemsize;
    v.format = if flags & pyo3::ffi::PyBUF_FORMAT == pyo3::ffi::PyBUF_FORMAT {
      slf.format.as_ptr().cast_mut()
    } else {
      std::ptr::null_mut()
    };
    v.ndim = slf.ndim;
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

//! Video nodes and frame iteration.

use std::collections::VecDeque;
use std::ffi::CString;
use std::sync::{Arc, LazyLock};

use async_executor::Executor;
use num_rational::Ratio;
use parking_lot::Mutex;
use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyStopIteration, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PySlice, PySliceMethods};
use vapoursynth4_rs::ColorFamily;
use vapoursynth4_rs::frame::{AudioFrame, VideoFrame};
use vapoursynth4_rs::node::{AudioNode, FrameRequest, Node, VideoNode};

use crate::core::OwnerCell;
use crate::enums::SampleType;
use crate::environment;
use crate::frame::{PyAudioFrame, PyVideoFormat, PyVideoFrame};

/// Represents a video clip.
#[pyclass(name = "VideoNode", frozen)]
pub(crate) struct PyVideoNode {
  pub(crate) node: VideoNode,
  pub(crate) owner: Arc<OwnerCell>,
}

#[pymethods]
impl PyVideoNode {
  /// The width of the video. This value will be 0 if the width and height can
  /// change between frames.
  #[getter]
  fn width(&self) -> i32 {
    self.node.info().width
  }

  /// The height of the video. This value will be 0 if the width and height can
  /// change between frames.
  #[getter]
  fn height(&self) -> i32 {
    self.node.info().height
  }

  /// The number of frames in the clip.
  #[getter]
  fn num_frames(&self) -> i32 {
    self.node.info().num_frames
  }

  /// The framerate represented as a Fraction. It is 0/1 when the clip has a
  /// variable framerate.
  #[getter]
  fn fps(&self) -> Ratio<i64> {
    let info = self.node.info();
    Ratio::new_raw(info.fps_num, info.fps_den)
  }

  /// A `VideoFormat` describing the clip's frame data. `None` when the format
  /// can change between frames.
  #[getter]
  fn format(&self, py: Python<'_>) -> PyResult<Option<Py<PyVideoFormat>>> {
    let format = &self.node.info().format;
    if format.color_family == ColorFamily::Undefined {
      return Ok(None);
    }
    Py::new(py, PyVideoFormat::from_vs(format, &self.owner)).map(Some)
  }

  fn __len__(&self) -> usize {
    self.node.info().num_frames.max(0) as usize
  }

  /// Returns a `VideoFrame` from position n.
  fn get_frame(&self, py: Python<'_>, n: i32) -> PyResult<Py<PyVideoFrame>> {
    let node = &self.node;
    let frame = py
      .detach(|| node.get_frame(n))
      .map_err(|e| PyRuntimeError::new_err(e.to_string_lossy().into_owned()))?;
    PyVideoFrame::create(py, frame, self.owner.clone())
  }

  /// Renders frame `n` concurrently in the core's thread pool. Returns a
  /// coroutine resolving to the `VideoFrame`.
  async fn get_frame_async(&self, n: i32) -> PyResult<Py<PyVideoFrame>> {
    let frame = FRAME_EXECUTOR
      .spawn(self.node.get_frame_async(n))
      .await
      .map_err(|e| PyRuntimeError::new_err(e.to_string_lossy().into_owned()))?;
    Python::attach(|py| PyVideoFrame::create(py, frame, self.owner.clone()))
  }

  /// Returns a generator iterator of all `VideoFrame`s in the clip. It will
  /// render multiple frames concurrently.
  ///
  /// `prefetch` is the number of frames to render concurrently, defaulting to
  /// the core's thread count. `backlog` is how many unconsumed frames may be
  /// buffered ahead of the consumer, defaulting to `prefetch * 3`.
  #[pyo3(signature = (prefetch=None, backlog=None))]
  fn frames(&self, prefetch: Option<i32>, backlog: Option<i32>) -> PyVideoFrameIter {
    PyVideoFrameIter {
      core: FrameIterCore::new(
        NodeKind::Video(self.node.clone()),
        self.owner.clone(),
        self.node.info().num_frames,
        prefetch,
        backlog,
      ),
    }
  }

  /// Registers this clip as an output on the current environment.
  #[pyo3(signature = (index = 0, alpha = None, alt_output = 0))]
  fn set_output(
    slf: Py<Self>,
    py: Python<'_>,
    index: i32,
    alpha: Option<Py<Self>>,
    alt_output: i32,
  ) -> PyResult<()> {
    if let Some(alpha) = &alpha {
      let main = slf.borrow(py);
      let main = main.node.info();
      let alpha = alpha.borrow(py);
      let alpha = alpha.node.info();

      if main.width != alpha.width || main.height != alpha.height {
        return Err(PyRuntimeError::new_err(
          "Alpha clip dimensions must match the main video",
        ));
      }
      if main.num_frames != alpha.num_frames {
        return Err(PyRuntimeError::new_err(
          "Alpha clip length must match the main video",
        ));
      }

      let main_known = main.format.color_family != ColorFamily::Undefined;
      let alpha_known = alpha.format.color_family != ColorFamily::Undefined;
      if main_known && alpha_known {
        if alpha.format.color_family != ColorFamily::Gray
          || alpha.format.sample_type != main.format.sample_type
          || alpha.format.bits_per_sample != main.format.bits_per_sample
        {
          return Err(PyRuntimeError::new_err(
            "Alpha clip format must match the main video",
          ));
        }
      } else if main_known || alpha_known {
        return Err(PyRuntimeError::new_err(
          "Format must be either known or unknown for both alpha and main clip",
        ));
      }
    }

    environment::store_video_output(py, index, slf, alpha, alt_output)
  }

  fn __repr__(&self) -> String {
    let info = self.node.info();
    format!(
      "<rynth.VideoNode {}x{}, {} frames, {}/{} fps>",
      info.width, info.height, info.num_frames, info.fps_num, info.fps_den
    )
  }

  #[allow(clippy::needless_pass_by_value)]
  fn __add__(&self, py: Python<'_>, other: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
    let node = self
      .owner
      .std()
      .splice()
      .clips(vec![self.node.clone(), other.node.clone()])
      .call()?;
    self.wrap_node(py, node)
  }

  fn __mul__(&self, py: Python<'_>, n: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let n: i64 = n
      .extract()
      .map_err(|_| PyTypeError::new_err("Clips may only be repeated by integer factors"))?;
    if n <= 0 {
      return Err(PyValueError::new_err("Loop count must greater than zero"));
    }
    let node = self
      .owner
      .std()
      .repeat()
      .clip(self.node.clone())
      .times(n)
      .call()?;
    self.wrap_node(py, node)
  }

  #[allow(clippy::needless_pass_by_value)]
  fn __getitem__(&self, py: Python<'_>, index: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let len = i64::from(self.node.info().num_frames);

    // `clip[start:stop:step]`
    if let Ok(slice) = index.cast::<PySlice>() {
      let indices = slice.indices(len as isize)?;
      let step = indices.step;
      let (mut first, mut last) = if step > 0 {
        (indices.start, indices.stop)
      } else {
        (indices.stop, indices.start)
      };

      // Make bounds inclusive for std.Trim.
      if step > 0 {
        last -= 1;
      } else {
        first += 1;
      }

      let mut node = self
        .owner
        .std()
        .trim()
        .clip(self.node.clone())
        .first(first as i64)
        .last(last as i64)
        .call()?;
      if step < 0 {
        node = self.owner.std().reverse().clip(node).call()?;
      }
      if step.abs() != 1 {
        node = self
          .owner
          .std()
          .select_every()
          .clip(node)
          .cycle(step.unsigned_abs() as i64)
          .offset(0)
          .call()?;
      }
      return self.wrap_node(py, node);
    }

    // `clip[n]`
    let n: i64 = index
      .extract()
      .map_err(|_| PyTypeError::new_err("VideoNode indices must be integers or slices"))?;
    let frame = if n < 0 { n + len } else { n };
    if frame < 0 || frame >= len {
      return Err(PyIndexError::new_err("VideoNode index out of range"));
    }
    let node = self
      .owner
      .std()
      .trim()
      .clip(self.node.clone())
      .first(frame)
      .last(frame)
      .call()?;
    self.wrap_node(py, node)
  }
}

impl PyVideoNode {
  /// Wraps a raw node into a Python `VideoNode` sharing this clip's core.
  fn wrap_node(&self, py: Python<'_>, node: VideoNode) -> PyResult<Py<PyAny>> {
    Ok(
      Py::new(
        py,
        Self {
          node,
          owner: self.owner.clone(),
        },
      )?
      .into_any(),
    )
  }
}

/// Represents an audio clip.
#[pyclass(name = "AudioNode", frozen)]
pub(crate) struct PyAudioNode {
  pub(crate) node: AudioNode,
  pub(crate) owner: Arc<OwnerCell>,
}

#[pymethods]
impl PyAudioNode {
  /// Whether the samples are integers or floats.
  #[getter]
  fn sample_type(&self) -> SampleType {
    self.node.info().format.sample_type.into()
  }

  /// Number of significant bits per sample.
  #[getter]
  fn bits_per_sample(&self) -> i32 {
    self.node.info().format.bits_per_sample
  }

  /// Number of bytes needed to store one sample.
  #[getter]
  fn bytes_per_sample(&self) -> i32 {
    self.node.info().format.bytes_per_sample
  }

  /// Bitmask of the channels present in the clip.
  #[getter]
  fn channel_layout(&self) -> u64 {
    self.node.info().format.channel_layout
  }

  /// Number of audio channels.
  #[getter]
  fn num_channels(&self) -> i32 {
    self.node.info().format.num_channels
  }

  /// The sample rate in Hz.
  #[getter]
  fn sample_rate(&self) -> i32 {
    self.node.info().sample_rate
  }

  /// Length of the clip in audio samples.
  #[getter]
  fn num_samples(&self) -> i64 {
    self.node.info().num_samples
  }

  /// Length of the clip in audio frames.
  #[getter]
  fn num_frames(&self) -> i32 {
    self.node.info().num_frames
  }

  fn __len__(&self) -> usize {
    self.node.info().num_frames.max(0) as usize
  }

  /// Returns an `AudioFrame` from position n.
  fn get_frame(&self, py: Python<'_>, n: i32) -> PyResult<Py<PyAudioFrame>> {
    let node = &self.node;
    let frame = py
      .detach(|| node.get_frame(n))
      .map_err(|e| PyRuntimeError::new_err(e.to_string_lossy().into_owned()))?;
    PyAudioFrame::create(py, frame, self.owner.clone())
  }

  /// Renders frame `n` concurrently in the core's thread pool. Returns a
  /// coroutine resolving to the `AudioFrame`.
  async fn get_frame_async(&self, n: i32) -> PyResult<Py<PyAudioFrame>> {
    let frame = FRAME_EXECUTOR
      .spawn(self.node.get_frame_async(n))
      .await
      .map_err(|e| PyRuntimeError::new_err(e.to_string_lossy().into_owned()))?;
    Python::attach(|py| PyAudioFrame::create(py, frame, self.owner.clone()))
  }

  /// Returns a generator iterator of all `AudioFrame`s in the clip. It will
  /// render multiple frames concurrently.
  #[pyo3(signature = (prefetch=None, backlog=None))]
  fn frames(&self, prefetch: Option<i32>, backlog: Option<i32>) -> PyAudioFrameIter {
    PyAudioFrameIter {
      core: FrameIterCore::new(
        NodeKind::Audio(self.node.clone()),
        self.owner.clone(),
        self.node.info().num_frames,
        prefetch,
        backlog,
      ),
    }
  }

  /// Registers this clip as an output on the current environment.
  #[pyo3(signature = (index = 0))]
  fn set_output(slf: Py<Self>, py: Python<'_>, index: i32) -> PyResult<()> {
    environment::store_audio_output(py, index, slf)
  }

  fn __repr__(&self) -> String {
    let info = self.node.info();
    format!(
      "<rynth.AudioNode {} Hz, {} channels, {} samples>",
      info.sample_rate, info.format.num_channels, info.num_samples
    )
  }
}

/// The shared executor that drives async frame requests.
static FRAME_EXECUTOR: LazyLock<Arc<Executor<'static>>> = LazyLock::new(|| {
  let executor = Arc::new(Executor::new());
  let driver = Arc::clone(&executor);
  std::thread::Builder::new()
    .name("rynth-frame-async".to_owned())
    .spawn(move || futures_lite::future::block_on(driver.run(std::future::pending::<()>())))
    .expect("failed to spawn rynth frame executor thread");
  executor
});

/// A node whose frames the iterator renders.
enum NodeKind {
  Video(VideoNode),
  Audio(AudioNode),
}

/// An in-flight request for a frame.
enum FrameReq {
  Video(FrameRequest<VideoFrame>),
  Audio(FrameRequest<AudioFrame>),
}

/// A rendered frame.
enum FrameOut {
  Video(VideoFrame),
  Audio(AudioFrame),
}

impl FrameReq {
  fn try_recv(&mut self) -> Option<Result<FrameOut, CString>> {
    match self {
      Self::Video(req) => req.try_recv().map(|r| r.map(FrameOut::Video)),
      Self::Audio(req) => req.try_recv().map(|r| r.map(FrameOut::Audio)),
    }
  }

  fn recv_blocking(self) -> Result<FrameOut, CString> {
    match self {
      Self::Video(req) => req.recv_blocking().map(FrameOut::Video),
      Self::Audio(req) => req.recv_blocking().map(FrameOut::Audio),
    }
  }
}

/// An in-flight or completed frame request.
enum Slot {
  Rendering(FrameReq),
  Done(Result<FrameOut, CString>),
}

struct IterState {
  /// Outstanding requests in frame order; the consumer pops from the front.
  window: VecDeque<Slot>,
  /// Next frame number to request.
  next_request: i32,
  /// No more requests will be issued (a frame failed).
  stopped: bool,
}

/// Frame iterator that keeps up to `prefetch` frames rendering concurrently
/// while never buffering more than `backlog` unconsumed frames.
struct FrameIterCore {
  node: NodeKind,
  owner: Arc<OwnerCell>,
  len: i32,
  prefetch: usize,
  backlog: usize,
  state: Mutex<IterState>,
}

impl FrameIterCore {
  fn new(
    node: NodeKind,
    owner: Arc<OwnerCell>,
    len: i32,
    prefetch: Option<i32>,
    backlog: Option<i32>,
  ) -> Self {
    let prefetch = match prefetch {
      Some(p) if p > 0 => p as usize,
      _ => owner.with_core(|core| core.get_info().num_threads).max(1) as usize,
    };
    let backlog = match backlog {
      Some(b) if b >= 0 => (b as usize).max(prefetch),
      _ => prefetch * 3,
    };
    let core = Self {
      node,
      owner,
      len,
      prefetch,
      backlog,
      state: Mutex::new(IterState {
        window: VecDeque::new(),
        next_request: 0,
        stopped: false,
      }),
    };
    // Kick off the initial burst of requests.
    core.refill(&mut core.state.lock());
    core
  }

  /// Issue new requests while below the concurrency and backlog limits.
  fn refill(&self, st: &mut IterState) {
    if st.stopped {
      return;
    }
    // Settle finished renders so they count against `backlog` but free up
    // `prefetch` capacity.
    let mut rendering = 0;
    for slot in &mut st.window {
      if let Slot::Rendering(req) = slot {
        if let Some(result) = req.try_recv() {
          // Stop requesting past a failure, but keep the window so earlier
          // frames still yield first.
          st.stopped |= result.is_err();
          *slot = Slot::Done(result);
        } else {
          rendering += 1;
        }
      }
    }
    while !st.stopped
      && st.next_request < self.len
      && rendering < self.prefetch
      && st.window.len() < self.backlog
    {
      let req = match &self.node {
        NodeKind::Video(node) => FrameReq::Video(node.get_frame_async(st.next_request)),
        NodeKind::Audio(node) => FrameReq::Audio(node.get_frame_async(st.next_request)),
      };
      st.next_request += 1;
      st.window.push_back(Slot::Rendering(req));
      rendering += 1;
    }
  }

  /// Blocks (without the GIL) until the next frame in order is rendered.
  /// `None` signals exhaustion.
  fn next(&self, py: Python<'_>) -> PyResult<Option<FrameOut>> {
    py.detach(|| {
      let mut st = self.state.lock();
      let Some(slot) = st.window.pop_front() else {
        return Ok(None);
      };
      let result = match slot {
        Slot::Done(result) => result,
        Slot::Rendering(req) => req.recv_blocking(),
      };
      match result {
        Ok(frame) => {
          self.refill(&mut st);
          Ok(Some(frame))
        }
        Err(msg) => {
          st.stopped = true;
          drain(&mut st.window);
          drop(st);
          Err(msg.to_string_lossy().into_owned())
        }
      }
    })
    .map_err(PyRuntimeError::new_err)
  }
}

impl Drop for FrameIterCore {
  fn drop(&mut self) {
    drain(&mut self.state.get_mut().window);
  }
}

/// Wait out every in-flight request, discarding the results.
///
/// Requests already handed to the core cannot be cancelled. Dropping their
/// futures would let the core keep rendering after the consumer (and
/// eventually the core itself) is gone, aborting the process.
fn drain(window: &mut VecDeque<Slot>) {
  for slot in window.drain(..) {
    if let Slot::Rendering(req) = slot {
      let _ = req.recv_blocking();
    }
  }
}

/// Iterator over a video clip's frames, yielding `VideoFrame`s in order and
/// rendering multiple frames concurrently.
#[pyclass(name = "VideoFrameIter", frozen)]
pub(crate) struct PyVideoFrameIter {
  core: FrameIterCore,
}

#[pymethods]
impl PyVideoFrameIter {
  const fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
    slf
  }

  fn __next__(&self, py: Python<'_>) -> PyResult<Py<PyVideoFrame>> {
    match self.core.next(py)? {
      Some(FrameOut::Video(frame)) => PyVideoFrame::create(py, frame, self.core.owner.clone()),
      Some(FrameOut::Audio(_)) => unreachable!("video iterator only renders video frames"),
      None => Err(PyStopIteration::new_err(())),
    }
  }
}

/// Iterator over an audio clip's frames, yielding `AudioFrame`s in order and
/// rendering multiple frames concurrently.
#[pyclass(name = "AudioFrameIter", frozen)]
pub(crate) struct PyAudioFrameIter {
  core: FrameIterCore,
}

#[pymethods]
impl PyAudioFrameIter {
  const fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
    slf
  }

  fn __next__(&self, py: Python<'_>) -> PyResult<Py<PyAudioFrame>> {
    match self.core.next(py)? {
      Some(FrameOut::Audio(frame)) => PyAudioFrame::create(py, frame, self.core.owner.clone()),
      Some(FrameOut::Video(_)) => unreachable!("audio iterator only renders audio frames"),
      None => Err(PyStopIteration::new_err(())),
    }
  }
}

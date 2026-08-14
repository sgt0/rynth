//! Video nodes and frame iteration.

use std::collections::VecDeque;
use std::ffi::CString;
use std::sync::Arc;

use num_rational::Ratio;
use parking_lot::Mutex;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use vapoursynth4_rs::ColorFamily;
use vapoursynth4_rs::frame::VideoFrame;
use vapoursynth4_rs::node::{FrameRequest, Node, VideoNode};

use crate::core::OwnerCell;
use crate::environment;
use crate::frame::PyVideoFrame;

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

  #[getter]
  fn format_name(&self) -> Option<String> {
    let format = &self.node.info().format;
    self
      .owner
      .with_core(|core| core.get_video_format_name(format))
      .map(crate::frame::trim_format_name)
  }

  fn __len__(&self) -> usize {
    self.node.info().num_frames.max(0) as usize
  }

  /// Returns a `VideoFrame` from position n.
  fn get_frame(&self, py: Python<'_>, n: i32) -> PyResult<PyVideoFrame> {
    let node = &self.node;
    let frame = py
      .detach(|| node.get_frame(n))
      .map_err(|e| PyRuntimeError::new_err(e.to_string_lossy().into_owned()))?;
    Ok(PyVideoFrame::new(frame, self.owner.clone()))
  }

  /// Renders frame n concurrently in the core's thread pool. Returns a
  /// coroutine resolving to the `VideoFrame`.
  ///
  /// The frame request is issued once the coroutine is first polled (e.g.
  /// when awaited or scheduled as a task), so `asyncio.gather` renders
  /// multiple frames concurrently.
  async fn get_frame_async(&self, n: i32) -> PyResult<PyVideoFrame> {
    let node = self.node.clone();
    let owner = self.owner.clone();
    let frame = node
      .get_frame_async(n)
      .await
      .map_err(|e| PyRuntimeError::new_err(e.to_string_lossy().into_owned()))?;
    Ok(PyVideoFrame::new(frame, owner))
  }

  /// Returns a generator iterator of all `VideoFrame`s in the clip. It will
  /// render multiple frames concurrently.
  ///
  /// `prefetch` is the number of frames to render concurrently, defaulting to
  /// the core's thread count. `backlog` is how many unconsumed frames may be
  /// buffered ahead of the consumer, defaulting to `prefetch * 3`.
  #[pyo3(signature = (prefetch=None, backlog=None))]
  fn frames(&self, prefetch: Option<i32>, backlog: Option<i32>) -> PyFrameIter {
    let prefetch = match prefetch {
      Some(p) if p > 0 => p as usize,
      _ => self
        .owner
        .with_core(|core| core.get_info().num_threads)
        .max(1) as usize,
    };
    let backlog = match backlog {
      Some(b) if b >= 0 => (b as usize).max(prefetch),
      _ => prefetch * 3,
    };

    let iter = PyFrameIter {
      node: self.node.clone(),
      owner: self.owner.clone(),
      len: self.node.info().num_frames,
      prefetch,
      backlog,
      state: Mutex::new(IterState {
        window: VecDeque::new(),
        next_request: 0,
        stopped: false,
      }),
    };
    // Kick off the initial burst of requests.
    iter.refill(&mut iter.state.lock());
    iter
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
}

/// An in-flight or completed frame request.
enum Slot {
  Rendering(FrameRequest<VideoFrame>),
  Done(Result<VideoFrame, CString>),
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
#[pyclass(name = "FrameIter", frozen)]
pub(crate) struct PyFrameIter {
  node: VideoNode,
  owner: Arc<OwnerCell>,
  len: i32,
  prefetch: usize,
  backlog: usize,
  state: Mutex<IterState>,
}

impl PyFrameIter {
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
      let req = self.node.get_frame_async(st.next_request);
      st.next_request += 1;
      st.window.push_back(Slot::Rendering(req));
      rendering += 1;
    }
  }
}

#[pymethods]
impl PyFrameIter {
  const fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
    slf
  }

  fn __next__(&self, py: Python<'_>) -> PyResult<Option<PyVideoFrame>> {
    // Block (without the GIL) until the next frame in order is rendered.
    let frame = py.detach(|| {
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
    });

    Ok(
      frame
        .map_err(PyRuntimeError::new_err)?
        .map(|frame| PyVideoFrame::new(frame, self.owner.clone())),
    )
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

impl Drop for PyFrameIter {
  fn drop(&mut self) {
    drain(&mut self.state.get_mut().window);
  }
}

//! Convenience helpers for building and reading maps.

use std::ffi::CStr;

use pyo3::PyResult;
use pyo3::exceptions::PyRuntimeError;
use vapoursynth4_rs::map::{AppendMode, KeyStr, Map, Value};
use vapoursynth4_rs::node::VideoNode;

/// Convenience helpers for building and reading VapourSynth maps.
pub(crate) trait MapExt {
  /// Sets `key` to an integer, replacing any existing value.
  fn set_int(&mut self, key: &CStr, value: i64) -> PyResult<()>;
  /// Sets `key` to a video node, replacing any existing value.
  fn set_node(&mut self, key: &CStr, node: VideoNode) -> PyResult<()>;
  /// Appends a video node to the array under `key`.
  fn push_node(&mut self, key: &CStr, node: VideoNode) -> PyResult<()>;
  /// Reads the first video node under `key`.
  fn get_node(&self, key: &CStr) -> PyResult<VideoNode>;
}

impl MapExt for Map {
  fn set_int(&mut self, key: &CStr, value: i64) -> PyResult<()> {
    self
      .set(
        KeyStr::from_cstr(key),
        Value::Int(value),
        AppendMode::Replace,
      )
      .map_err(|e| PyRuntimeError::new_err(e.to_string()))
  }

  fn set_node(&mut self, key: &CStr, node: VideoNode) -> PyResult<()> {
    self
      .set(
        KeyStr::from_cstr(key),
        Value::VideoNode(node),
        AppendMode::Replace,
      )
      .map_err(|e| PyRuntimeError::new_err(e.to_string()))
  }

  fn push_node(&mut self, key: &CStr, node: VideoNode) -> PyResult<()> {
    self
      .set(
        KeyStr::from_cstr(key),
        Value::VideoNode(node),
        AppendMode::Append,
      )
      .map_err(|e| PyRuntimeError::new_err(e.to_string()))
  }

  fn get_node(&self, key: &CStr) -> PyResult<VideoNode> {
    match self.get(KeyStr::from_cstr(key), 0) {
      Ok(Value::VideoNode(node)) => Ok(node),
      Ok(_) => Err(PyRuntimeError::new_err("expected a video node")),
      Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
    }
  }
}

//! Bidirectional conversion between Python values and `VSMap` entries.

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::Arc;

use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use vapoursynth4_rs::map::{AppendMode, KeyStr, Map, Value};

use crate::core::OwnerCell;
use crate::frame::PyVideoFrame;
use crate::node::PyVideoNode;

pub(crate) fn map_key(name: &str) -> PyResult<CString> {
  if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
    return Err(PyTypeError::new_err(format!(
      "invalid argument name: {name:?}"
    )));
  }
  Ok(CString::new(name).expect("validated above"))
}

/// Parses a VS plugin function signature ("width:int:opt;color:float[]:opt")
/// into parameter name and type.
pub(crate) fn signature_types(signature: &str) -> HashMap<String, String> {
  signature
    .split(';')
    .filter(|p| !p.is_empty())
    .filter_map(|p| {
      let mut parts = p.split(':');
      let name = parts.next()?;
      let ty = parts.next()?;
      Some((name.to_string(), ty.trim_end_matches("[]").to_string()))
    })
    .collect()
}

fn set_scalar(
  map: &mut Map,
  key: &KeyStr,
  value: &Bound<'_, PyAny>,
  append: AppendMode,
  want_float: bool,
) -> PyResult<()> {
  let val = if let Ok(b) = value.extract::<bool>() {
    let i = i64::from(b);
    if want_float {
      Value::Float(i as f64)
    } else {
      Value::Int(i)
    }
  } else if let Ok(i) = value.extract::<i64>() {
    if want_float {
      Value::Float(i as f64)
    } else {
      Value::Int(i)
    }
  } else if let Ok(f) = value.extract::<f64>() {
    Value::Float(f)
  } else if let Ok(s) = value.extract::<String>() {
    return map
      .set(key, Value::Utf8(&s), append)
      .map_err(|e| PyRuntimeError::new_err(e.to_string()));
  } else if let Ok(b) = value.extract::<Vec<u8>>() {
    return map
      .set(key, Value::Data(&b), append)
      .map_err(|e| PyRuntimeError::new_err(e.to_string()));
  } else if let Ok(node) = value.extract::<PyRef<'_, PyVideoNode>>() {
    Value::VideoNode(node.node.clone())
  } else {
    return Err(PyTypeError::new_err(format!(
      "unsupported argument type: {}",
      value.get_type().name()?
    )));
  };
  map
    .set(key, val, append)
    .map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

pub(crate) fn kwargs_to_map(
  map: &mut Map,
  kwargs: &Bound<'_, PyDict>,
  types: &HashMap<String, String>,
) -> PyResult<()> {
  for (k, v) in kwargs.iter() {
    let name: String = k.extract()?;
    let key_c = map_key(&name)?;
    let key = KeyStr::from_cstr(&key_c);
    let want_float = types.get(&name).is_some_and(|t| t == "float");
    if let Ok(list) = v.cast::<PyList>() {
      let mut mode = AppendMode::Replace;
      for item in list.iter() {
        set_scalar(map, key, &item, mode, want_float)?;
        mode = AppendMode::Append;
      }
    } else {
      set_scalar(map, key, &v, AppendMode::Replace, want_float)?;
    }
  }
  Ok(())
}

fn value_to_py(py: Python<'_>, value: Value<'_>, owner: &Arc<OwnerCell>) -> PyResult<Py<PyAny>> {
  Ok(match value {
    Value::Int(i) => i.into_pyobject(py)?.into_any().unbind(),
    Value::Float(f) => f.into_pyobject(py)?.into_any().unbind(),
    Value::Utf8(s) => s.into_pyobject(py)?.into_any().unbind(),
    Value::Data(d) => d.into_pyobject(py)?.into_any().unbind(),
    Value::VideoNode(node) => Py::new(
      py,
      PyVideoNode {
        node,
        owner: owner.clone(),
      },
    )?
    .into_any(),
    Value::VideoFrame(frame) => Py::new(py, PyVideoFrame::new(frame, owner.clone()))?.into_any(),
    Value::AudioNode(_) | Value::AudioFrame(_) => {
      return Err(PyRuntimeError::new_err(
        "audio values are not supported yet",
      ));
    }
    Value::Function(_) => {
      return Err(PyRuntimeError::new_err(
        "function values are not supported yet",
      ));
    }
  })
}

pub(crate) fn key_to_py(
  py: Python<'_>,
  map: &Map,
  key: &KeyStr,
  owner: &Arc<OwnerCell>,
) -> PyResult<Py<PyAny>> {
  let n = map.num_elements(key).unwrap_or(0);
  let mut values = Vec::with_capacity(n as usize);
  for i in 0..n {
    let value = map
      .get(key, i)
      .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    values.push(value_to_py(py, value, owner)?);
  }
  if values.len() == 1 {
    Ok(values.pop().expect("len checked"))
  } else {
    Ok(PyList::new(py, values)?.into_any().unbind())
  }
}

pub(crate) fn map_to_py_dict(
  py: Python<'_>,
  map: &Map,
  owner: &Arc<OwnerCell>,
) -> PyResult<Py<PyDict>> {
  let dict = PyDict::new(py);
  for i in 0..map.len() {
    let key = map.get_key(i);
    dict.set_item(key.to_string(), key_to_py(py, map, key, owner)?)?;
  }
  Ok(dict.unbind())
}

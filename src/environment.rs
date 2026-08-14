//! Environments and the pluggable environment-policy system.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use crate::core::PyCore;
use crate::node::PyVideoNode;

/// The currently registered policy, if any.
static POLICY: Mutex<Option<Py<PyAny>>> = Mutex::new(None);

/// A registered output.
#[pyclass(frozen, name = "VideoOutputTuple", module = "rynth")]
pub(crate) struct VideoOutputTuple {
  /// A VideoNode-instance containing the color planes.
  #[pyo3(get)]
  clip: Py<PyVideoNode>,
  /// A VideoNode-instance containing the alpha planes.
  #[pyo3(get)]
  alpha: Option<Py<PyVideoNode>>,
  /// An integer with the alternate output mode to be used. May be ignored if
  /// no meaningful mapping exists.
  #[pyo3(get)]
  alt_output: i32,
}

impl VideoOutputTuple {
  fn as_tuple<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
    let alpha = self
      .alpha
      .as_ref()
      .map_or_else(|| py.None(), |a| a.clone_ref(py).into_any());
    PyTuple::new(
      py,
      [
        self.clip.clone_ref(py).into_any(),
        alpha,
        self.alt_output.into_pyobject(py)?.into_any().unbind(),
      ],
    )
  }
}

#[pymethods]
impl VideoOutputTuple {
  #[allow(clippy::unused_self)]
  const fn __len__(&self) -> usize {
    3
  }

  fn __getitem__(&self, py: Python<'_>, idx: isize) -> PyResult<Py<PyAny>> {
    Ok(self.as_tuple(py)?.into_any().get_item(idx)?.unbind())
  }

  fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
    Ok(
      self
        .as_tuple(py)?
        .into_any()
        .try_iter()?
        .into_any()
        .unbind(),
    )
  }

  fn __repr__(&self) -> String {
    format!(
      "VideoOutputTuple(clip=<VideoNode>, alpha={}, alt_output={})",
      if self.alpha.is_some() {
        "<VideoNode>"
      } else {
        "None"
      },
      self.alt_output
    )
  }
}

/// Opaque per-environment state.
#[pyclass(frozen, name = "EnvironmentData", module = "rynth")]
pub(crate) struct EnvironmentData {
  alive: AtomicBool,
  #[allow(dead_code)]
  flags: i32,
  core: Mutex<Option<Py<PyCore>>>,
  outputs: Mutex<BTreeMap<i32, Py<VideoOutputTuple>>>,
}

impl EnvironmentData {
  const fn new(flags: i32) -> Self {
    Self {
      alive: AtomicBool::new(true),
      flags,
      core: Mutex::new(None),
      outputs: Mutex::new(BTreeMap::new()),
    }
  }

  /// Returns this environment's core, creating it on first use.
  fn get_or_create_core(&self, py: Python<'_>) -> PyResult<Py<PyCore>> {
    let mut guard = self.core.lock();
    if let Some(core) = guard.as_ref() {
      return Ok(core.clone_ref(py));
    }
    let core = Py::new(py, PyCore::new(None, None)?)?;
    *guard = Some(core.clone_ref(py));
    drop(guard);
    Ok(core)
  }

  /// Tears the environment down. Drops the core reference and clears outputs.
  /// Idempotent.
  fn destroy(&self) {
    self.alive.store(false, Ordering::SeqCst);
    self.outputs.lock().clear();
    // Drop this environment's reference to the core.
    let _ = self.core.lock().take();
  }
}

#[pymethods]
impl EnvironmentData {
  #[getter]
  fn alive(&self) -> bool {
    self.alive.load(Ordering::SeqCst)
  }
}

/// Base class for pluggable environment policies. Subclass in Python and pass
/// an instance to [`register_policy`] to control how the current environment is
/// selected (e.g. per-thread for a script host).
#[pyclass(subclass, name = "EnvironmentPolicy", module = "rynth")]
pub(crate) struct EnvironmentPolicy;

#[pymethods]
impl EnvironmentPolicy {
  #[new]
  const fn new() -> Self {
    Self
  }

  #[allow(clippy::unused_self)]
  fn on_policy_registered(&self, _special_api: Py<PyAny>) {}

  #[allow(clippy::unused_self)]
  const fn on_policy_cleared(&self) {}

  #[allow(clippy::unused_self)]
  fn get_current_environment(&self) -> PyResult<Py<PyAny>> {
    Err(PyNotImplementedError::new_err(()))
  }

  #[allow(clippy::unused_self)]
  fn set_environment(&self, _environment: Py<PyAny>) -> PyResult<Py<PyAny>> {
    Err(PyNotImplementedError::new_err(()))
  }

  #[allow(clippy::unused_self)]
  fn is_alive(&self, environment: Option<Py<EnvironmentData>>) -> bool {
    environment.is_some_and(|env| env.get().alive.load(Ordering::SeqCst))
  }
}

/// The default policy, which is an always-current single environment.
#[pyclass(frozen, name = "StandaloneEnvironmentPolicy", module = "rynth")]
pub(crate) struct StandaloneEnvironmentPolicy {
  environment: Mutex<Option<Py<EnvironmentData>>>,
  api: Mutex<Option<Py<EnvironmentPolicyAPI>>>,
}

impl StandaloneEnvironmentPolicy {
  const fn empty() -> Self {
    Self {
      environment: Mutex::new(None),
      api: Mutex::new(None),
    }
  }
}

#[pymethods]
impl StandaloneEnvironmentPolicy {
  fn on_policy_registered(&self, py: Python<'_>, api: Py<EnvironmentPolicyAPI>) -> PyResult<()> {
    let env = api.get().create_environment(py, 0)?;
    *self.environment.lock() = Some(env);
    *self.api.lock() = Some(api);
    Ok(())
  }

  fn on_policy_cleared(&self) {
    let api = self.api.lock().take();
    let env = self.environment.lock().take();
    if let (Some(api), Some(env)) = (api, env) {
      api.get().destroy_environment(env);
    }
  }

  fn get_current_environment(&self, py: Python<'_>) -> Option<Py<EnvironmentData>> {
    self.environment.lock().as_ref().map(|e| e.clone_ref(py))
  }

  fn set_environment(
    &self,
    py: Python<'_>,
    _environment: Py<PyAny>,
  ) -> Option<Py<EnvironmentData>> {
    self.environment.lock().as_ref().map(|e| e.clone_ref(py))
  }

  fn is_alive(&self, environment: &Bound<'_, PyAny>) -> bool {
    self
      .environment
      .lock()
      .as_ref()
      .is_some_and(|e| e.as_ptr() == environment.as_ptr())
  }
}

/// Handed to a policy on registration. Lets it create and destroy environments
/// and unregister itself.
#[pyclass(frozen, name = "EnvironmentPolicyAPI", module = "rynth")]
pub(crate) struct EnvironmentPolicyAPI {
  known: Mutex<Vec<Py<EnvironmentData>>>,
}

impl EnvironmentPolicyAPI {
  const fn empty() -> Self {
    Self {
      known: Mutex::new(Vec::new()),
    }
  }
}

#[pymethods]
impl EnvironmentPolicyAPI {
  #[pyo3(signature = (flags = 0))]
  fn create_environment(&self, py: Python<'_>, flags: i32) -> PyResult<Py<EnvironmentData>> {
    let env = Py::new(py, EnvironmentData::new(flags))?;
    self.known.lock().push(env.clone_ref(py));
    Ok(env)
  }

  #[allow(clippy::unused_self, clippy::needless_pass_by_value)]
  fn destroy_environment(&self, environment: Py<EnvironmentData>) {
    environment.get().destroy();
  }

  fn unregister_policy(&self, py: Python<'_>) -> PyResult<()> {
    let envs: Vec<Py<EnvironmentData>> = self.known.lock().drain(..).collect();
    for env in envs {
      env.get().destroy();
    }
    clear_policy_inner(py)
  }
}

/// A context manager that activates a target environment for the duration of a
/// `with` block, restoring the previous one on exit.
#[pyclass(name = "_FastManager", module = "rynth")]
pub(crate) struct FastManager {
  target: Option<Py<EnvironmentData>>,
  previous: Option<Py<EnvironmentData>>,
}

#[pymethods]
impl FastManager {
  fn __enter__(&mut self, py: Python<'_>) -> PyResult<()> {
    let policy = get_policy(py)?;
    let previous = policy.call_method0(py, "get_current_environment")?;
    self.previous = previous.bind(py).extract()?;
    if let Some(target) = self.target.take() {
      policy.call_method1(py, "set_environment", (target,))?;
    }
    Ok(())
  }

  fn __exit__(
    &mut self,
    py: Python<'_>,
    _exc_type: Py<PyAny>,
    _exc_value: Py<PyAny>,
    _traceback: Py<PyAny>,
  ) -> PyResult<bool> {
    let policy = get_policy(py)?;
    match self.previous.take() {
      Some(previous) if is_alive(py, &policy, &previous)? => {
        policy.call_method1(py, "set_environment", (previous,))?;
      }
      _ => {
        policy.call_method1(py, "set_environment", (py.None(),))?;
      }
    }
    Ok(false)
  }
}

/// A live handle to an environment. Use [`Environment::use_`] to activate it.
#[pyclass(frozen, name = "Environment", module = "rynth")]
pub(crate) struct Environment {
  env: Py<EnvironmentData>,
}

impl Environment {
  /// Returns True if the script is _not_ running inside a vsscript-Environment.
  /// If it is running inside a vsscript-Environment, it returns False.
  fn is_single(py: Python<'_>) -> bool {
    current_policy(py).is_none_or(|policy| {
      policy
        .bind(py)
        .is_instance_of::<StandaloneEnvironmentPolicy>()
    })
  }
}

#[pymethods]
impl Environment {
  #[getter]
  fn alive(&self, py: Python<'_>) -> PyResult<bool> {
    let policy = get_policy(py)?;
    is_alive(py, &policy, &self.env)
  }

  #[getter]
  #[allow(clippy::unused_self)]
  fn single(&self, py: Python<'_>) -> bool {
    Self::is_single(py)
  }

  #[getter]
  fn env_id(&self, py: Python<'_>) -> i64 {
    if Self::is_single(py) {
      -1
    } else {
      self.env.as_ptr() as i64
    }
  }

  #[getter]
  fn active(&self, py: Python<'_>) -> bool {
    env_current(py).is_some_and(|e| e.as_ptr() == self.env.as_ptr())
  }

  /// Returns a context-manager that enables the given environment in the block
  /// enclosed in the with-statement and restores the environment to the one
  /// defined before the with-block has been encountered.
  #[pyo3(name = "use")]
  fn use_(&self, py: Python<'_>) -> PyResult<FastManager> {
    if !self.env.get().alive.load(Ordering::SeqCst) {
      return Err(PyRuntimeError::new_err("The environment is dead."));
    }
    Ok(FastManager {
      target: Some(self.env.clone_ref(py)),
      previous: None,
    })
  }

  fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
    other
      .cast::<Self>()
      .is_ok_and(|o| o.borrow().env.as_ptr() == self.env.as_ptr())
  }

  fn __repr__(&self, py: Python<'_>) -> String {
    if Self::is_single(py) {
      "<Environment (default)>".to_owned()
    } else {
      let state = if self.env.get().alive.load(Ordering::SeqCst) {
        if self.active(py) { "active" } else { "alive" }
      } else {
        "dead"
      };
      format!("<Environment {} ({state})>", self.env.as_ptr() as usize)
    }
  }
}

fn current_policy(py: Python<'_>) -> Option<Py<PyAny>> {
  POLICY.lock().as_ref().map(|p| p.clone_ref(py))
}

/// Returns the active policy, registering a [`StandaloneEnvironmentPolicy`] if
/// none exists yet.
fn get_policy(py: Python<'_>) -> PyResult<Py<PyAny>> {
  if let Some(policy) = current_policy(py) {
    return Ok(policy);
  }
  let standalone: Py<PyAny> = Py::new(py, StandaloneEnvironmentPolicy::empty())?.into_any();
  register_policy_inner(py, &standalone)?;
  Ok(standalone)
}

fn register_policy_inner(py: Python<'_>, policy: &Py<PyAny>) -> PyResult<()> {
  {
    let mut guard = POLICY.lock();
    if guard.is_some() {
      return Err(PyRuntimeError::new_err(
        "There is already a policy registered.",
      ));
    }
    *guard = Some(policy.clone_ref(py));
  }
  // Call into Python without holding the lock to avoid re-entrant deadlock.
  let api = Py::new(py, EnvironmentPolicyAPI::empty())?;
  policy.call_method1(py, "on_policy_registered", (api,))?;
  Ok(())
}

fn clear_policy_inner(py: Python<'_>) -> PyResult<()> {
  let old = POLICY.lock().take();
  if let Some(policy) = old {
    policy.call_method0(py, "on_policy_cleared")?;
  }
  Ok(())
}

fn is_alive(py: Python<'_>, policy: &Py<PyAny>, env: &Py<EnvironmentData>) -> PyResult<bool> {
  policy
    .call_method1(py, "is_alive", (env.clone_ref(py),))?
    .bind(py)
    .extract()
}

/// The current environment, if any. Registers the standalone policy on demand.
fn env_current(py: Python<'_>) -> Option<Py<EnvironmentData>> {
  let policy = get_policy(py).ok()?;
  let current = policy.call_method0(py, "get_current_environment").ok()?;
  current.bind(py).extract().ok().flatten()
}

fn require_current_env(py: Python<'_>) -> PyResult<Py<EnvironmentData>> {
  env_current(py).ok_or_else(|| {
    PyRuntimeError::new_err(
      "No environment is currently activated. (Hint: get_current_environment().use() \
       selects an environment.)",
    )
  })
}

/// The current environment's core, creating it on first use.
pub(crate) fn current_core(py: Python<'_>) -> PyResult<Py<PyCore>> {
  require_current_env(py)?.get().get_or_create_core(py)
}

/// Whether the current environment already has a core.
pub(crate) fn core_resolved(py: Python<'_>) -> bool {
  let Some(policy) = current_policy(py) else {
    return false;
  };
  let Ok(current) = policy.call_method0(py, "get_current_environment") else {
    return false;
  };
  let Ok(Some(env)) = current.bind(py).extract::<Option<Py<EnvironmentData>>>() else {
    return false;
  };
  env.get().core.lock().is_some()
}

/// Stores a video output in the current environment's registry.
pub(crate) fn store_video_output(
  py: Python<'_>,
  index: i32,
  clip: Py<PyVideoNode>,
  alpha: Option<Py<PyVideoNode>>,
  alt_output: i32,
) -> PyResult<()> {
  let tuple = Py::new(
    py,
    VideoOutputTuple {
      clip,
      alpha,
      alt_output,
    },
  )?;
  require_current_env(py)?
    .get()
    .outputs
    .lock()
    .insert(index, tuple);
  Ok(())
}

/// Register an [`EnvironmentPolicy`]. Raises if one is already registered.
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn register_policy(py: Python<'_>, policy: Py<PyAny>) -> PyResult<()> {
  register_policy_inner(py, &policy)
}

/// Whether an environment policy is currently registered.
#[pyfunction]
pub(crate) fn has_policy() -> bool {
  POLICY.lock().is_some()
}

/// Clear the current policy, running its teardown. Mainly for hosts and tests.
#[pyfunction]
pub(crate) fn clear_policy(py: Python<'_>) -> PyResult<()> {
  clear_policy_inner(py)
}

/// The current environment as an [`Environment`] handle, ensuring its core
/// exists.
#[pyfunction]
pub(crate) fn get_current_environment(py: Python<'_>) -> PyResult<Environment> {
  let env = require_current_env(py)?;
  env.get().get_or_create_core(py)?;
  Ok(Environment { env })
}

/// The [`VideoOutputTuple`] registered at `index`. Raises `KeyError` if none.
#[pyfunction]
#[pyo3(signature = (index = 0))]
pub(crate) fn get_output(py: Python<'_>, index: i32) -> PyResult<Py<VideoOutputTuple>> {
  let env = require_current_env(py)?;
  let output = env
    .get()
    .outputs
    .lock()
    .get(&index)
    .map(|o| o.clone_ref(py));
  output.ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(index))
}

/// A read-only mapping of every registered output.
#[pyfunction]
pub(crate) fn get_outputs(py: Python<'_>) -> PyResult<Py<PyAny>> {
  let env = require_current_env(py)?;
  let dict = PyDict::new(py);
  {
    let data = env.get();
    let outputs = data.outputs.lock();
    for (index, output) in outputs.iter() {
      dict.set_item(*index, output.clone_ref(py))?;
    }
  }
  let mapping_proxy = py
    .import("types")?
    .getattr("MappingProxyType")?
    .call1((dict,))?;
  Ok(mapping_proxy.unbind())
}

/// Remove the output registered at `index`, if any.
#[pyfunction]
#[pyo3(signature = (index = 0))]
pub(crate) fn clear_output(py: Python<'_>, index: i32) -> PyResult<()> {
  require_current_env(py)?.get().outputs.lock().remove(&index);
  Ok(())
}

/// Remove every registered output.
#[pyfunction]
pub(crate) fn clear_outputs(py: Python<'_>) -> PyResult<()> {
  require_current_env(py)?.get().outputs.lock().clear();
  Ok(())
}

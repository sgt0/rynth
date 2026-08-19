//! Typed builders for the `std` plugin functions.

use bon::bon;
use pyo3::PyResult;
use vapoursynth4_rs::node::VideoNode;

use crate::core::OwnerCell;
use crate::map_ext::MapExt;

/// Typed access to the `std` plugin namespace of a core.
pub(crate) struct Std<'a>(pub(crate) &'a OwnerCell);

#[bon]
impl Std<'_> {
  /// `std.Trim`
  #[builder]
  pub(crate) fn trim(
    &self,
    clip: VideoNode,
    first: Option<i64>,
    last: Option<i64>,
  ) -> PyResult<VideoNode> {
    self
      .0
      .invoke(c"std", c"Trim", |args| {
        args.set_node(c"clip", clip)?;
        if let Some(first) = first {
          args.set_int(c"first", first)?;
        }
        if let Some(last) = last {
          args.set_int(c"last", last)?;
        }
        Ok(())
      })?
      .get_node(c"clip")
  }

  /// `std.Reverse`
  #[builder]
  pub(crate) fn reverse(&self, clip: VideoNode) -> PyResult<VideoNode> {
    self
      .0
      .invoke(c"std", c"Reverse", |args| args.set_node(c"clip", clip))?
      .get_node(c"clip")
  }

  /// `std.SelectEvery`
  #[builder]
  pub(crate) fn select_every(
    &self,
    clip: VideoNode,
    cycle: i64,
    offset: i64,
  ) -> PyResult<VideoNode> {
    self
      .0
      .invoke(c"std", c"SelectEvery", |args| {
        args.set_node(c"clip", clip)?;
        args.set_int(c"cycle", cycle)?;
        args.set_int(c"offsets", offset)
      })?
      .get_node(c"clip")
  }

  /// `std.Loop`
  #[builder]
  pub(crate) fn repeat(&self, clip: VideoNode, times: i64) -> PyResult<VideoNode> {
    self
      .0
      .invoke(c"std", c"Loop", |args| {
        args.set_node(c"clip", clip)?;
        args.set_int(c"times", times)
      })?
      .get_node(c"clip")
  }

  /// `std.Splice`
  #[builder]
  pub(crate) fn splice(&self, clips: Vec<VideoNode>) -> PyResult<VideoNode> {
    self
      .0
      .invoke(c"std", c"Splice", |args| {
        for clip in clips {
          args.push_node(c"clips", clip)?;
        }
        Ok(())
      })?
      .get_node(c"clip")
  }
}

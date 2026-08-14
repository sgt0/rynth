//! Locates `libvapoursynth.so.4` inside the installed `vapoursynth` wheel and
//! links against it.

use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

fn wheel_lib_dir() -> PathBuf {
  if let Ok(dir) = env::var("OXYSYNTH_LIB_DIR") {
    return PathBuf::from(dir);
  }
  let python = env::var("PYO3_PYTHON")
    .or_else(|_| env::var("VIRTUAL_ENV").map(|venv| format!("{venv}/bin/python")))
    .ok()
    .or_else(|| {
      let dev =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").ok()?).join(".devenv/state/venv/bin/python");
      dev.exists().then(|| dev.to_string_lossy().into_owned())
    })
    .unwrap_or_else(|| "python3".into());
  let out = Command::new(&python)
    .args([
      "-c",
      "import importlib.util; \
             print(importlib.util.find_spec('vapoursynth').submodule_search_locations[0])",
    ])
    .output()
    .expect("failed to run python to locate the vapoursynth wheel");
  assert!(
    out.status.success(),
    "could not locate the vapoursynth wheel (is it installed in the build env?): {}",
    String::from_utf8_lossy(&out.stderr)
  );
  PathBuf::from(
    String::from_utf8(out.stdout)
      .expect("non-UTF-8 path")
      .trim(),
  )
}

fn main() {
  println!("cargo:rerun-if-env-changed=OXYSYNTH_LIB_DIR");
  println!("cargo:rerun-if-env-changed=PYO3_PYTHON");
  println!("cargo:rerun-if-env-changed=VIRTUAL_ENV");

  let lib_dir = wheel_lib_dir();
  let versioned = lib_dir.join("libvapoursynth.so.4");
  assert!(
    versioned.exists(),
    "{} not found; the vapoursynth wheel layout changed?",
    versioned.display()
  );

  let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));
  let alias = out_dir.join("libvapoursynth.so");
  let _ = fs::remove_file(&alias);
  #[cfg(unix)]
  std::os::unix::fs::symlink(&versioned, &alias).expect("failed to create linker alias");

  println!("cargo:rustc-link-search=native={}", out_dir.display());
  println!("cargo:rustc-link-lib=dylib=vapoursynth");
  // site-packages/rynth/rynth*.so -> site-packages/vapoursynth/.
  println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../vapoursynth");
}

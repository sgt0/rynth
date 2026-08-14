{
  pkgs,
  lib,
  config,
  inputs,
  ...
}: {
  languages.python = {
    enable = true;
    uv = {
      enable = true;
      sync.enable = true;
    };
  };

  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
  };

  git-hooks.hooks = {
    alejandra.enable = true;
    cargo-check.enable = true;
    clippy.enable = true;
    ruff.enable = true;
    ruff-format.enable = true;
    rustfmt.enable = true;
  };

  packages = with pkgs; [
    pinact
  ];
}

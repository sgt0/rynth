#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."

uv run --frozen maturin generate-stubs --out stubs/rynth/
mv -f stubs/rynth/rynth.pyi stubs/rynth/__init__.pyi

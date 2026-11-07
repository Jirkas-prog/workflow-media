#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
project_dir="$PWD"
tool_dir="${CARGO_HOME:-$HOME/.cargo}"
# Keep the separator outside quotes so Cargo receives two independent options.
export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$project_dir=."$'\x1f'"--remap-path-prefix=$tool_dir=cargo"
cargo build --release --locked
mkdir -p Standalone
cp -- "${CARGO_TARGET_DIR:-target}/release/workflow-media" Standalone/workflow-media
chmod +x Standalone/workflow-media
sha256sum Standalone/workflow-media

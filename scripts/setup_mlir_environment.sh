#!/usr/bin/env bash
set -euo pipefail

# Default MLIR header path (system-installed)
HEADER="${MLIR_HEADER:-/usr/include/mlir-c/IR.h}"

if [ ! -f "$HEADER" ]; then
  echo "MLIR header not found at $HEADER"
  echo "Set MLIR_HEADER to the path of mlir-c/IR.h and rerun."
  exit 1
fi

INCLUDE="${MLIR_INCLUDE:-$(dirname "$HEADER")}"

export MLIR_HEADER="$HEADER"
export MLIR_INCLUDE="$INCLUDE"

echo "MLIR_HEADER=$MLIR_HEADER"
echo "MLIR_INCLUDE=$MLIR_INCLUDE"

# Build the crate with MLIR feature to trigger bindgen in build.rs
cargo build --features mlir
#!/usr/bin/env bash
set -euo pipefail
: "${CVVDP_REFERENCE:?Set CVVDP_REFERENCE to a local ColorVideoVDP checkout}"
: "${CVVDP_PYTHON:?Set CVVDP_PYTHON to a Python executable with the reference dependencies}"
root=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd -- "$root"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target}"
export PYTHONDONTWRITEBYTECODE=1 CUDA_VISIBLE_DEVICES=""
results="${CVVDP_RESULTS:-$root/parity/results/corpus}"
"$CVVDP_PYTHON" "$root/parity/reference.py" generate "$results"
"$CVVDP_PYTHON" "$root/parity/measure.py" "$results" single compare
"$CVVDP_PYTHON" "$root/parity/measure.py" "$results" parallel compare
"$CVVDP_PYTHON" "$root/parity/report.py" parity "$results" --output "$results/summary.md"

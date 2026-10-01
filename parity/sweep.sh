#!/usr/bin/env bash
set -euo pipefail
: "${CVVDP_REFERENCE:?Set CVVDP_REFERENCE to a local ColorVideoVDP checkout}"
: "${CVVDP_PYTHON:?Set CVVDP_PYTHON to a Python executable with the reference dependencies}"
root=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd -- "$root"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target}"
export PYTHONDONTWRITEBYTECODE=1 CUDA_VISIBLE_DEVICES=""
results="${CVVDP_SWEEP_RESULTS:-$root/parity/results/sweep}"
"$CVVDP_PYTHON" "$root/parity/reference.py" sweep "$results" --cases "${CVVDP_SWEEP_CASES:-500}" --seed "${CVVDP_SWEEP_SEED:-20261002}"
"$CVVDP_PYTHON" "$root/parity/measure.py" "$results" single compare
"$CVVDP_PYTHON" "$root/parity/measure.py" "$results" parallel compare
"$CVVDP_PYTHON" "$root/parity/sweep_report.py" "$results" --output "$root/parity/results/sweep.md"

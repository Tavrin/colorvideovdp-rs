#!/usr/bin/env bash
set -euo pipefail
: "${CVVDP_REFERENCE:?Set CVVDP_REFERENCE to a local ColorVideoVDP checkout}"
: "${CVVDP_PYTHON:?Set CVVDP_PYTHON to a Python executable with the reference dependencies}"
root=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd -- "$root"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target}"
export PYTHONDONTWRITEBYTECODE=1 CUDA_VISIBLE_DEVICES=""
export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-16}"
repeats="${CVVDP_REPEATS:-3}"
results="${CVVDP_BENCHMARK_RESULTS:-$root/parity/results/benchmark}"
"$CVVDP_PYTHON" "$root/parity/reference.py" generate-benchmark "$results"
"$CVVDP_PYTHON" "$root/parity/reference.py" benchmark "$results" --threads 1 --repeats "$repeats"
"$CVVDP_PYTHON" "$root/parity/measure.py" "$results" single benchmark --repeats "$repeats"
"$CVVDP_PYTHON" "$root/parity/reference.py" benchmark "$results" --threads "$RAYON_NUM_THREADS" --repeats "$repeats"
"$CVVDP_PYTHON" "$root/parity/measure.py" "$results" parallel benchmark --repeats "$repeats"

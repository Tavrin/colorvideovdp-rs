# Parity and benchmarks

This directory is a separate, unpublished Rust package. It compares the crate
with the official PyTorch implementation of ColorVideoVDP on CPU, and times
both. It is not part of the published `colorvideovdp` crate.

## Requirements

- A checkout of <https://github.com/gfxdisp/ColorVideoVDP> at revision
  `2a268bc`, given by `CVVDP_REFERENCE`. It is imported in place; nothing is
  installed into it or modified.
- A Python executable, given by `CVVDP_PYTHON`, with CPU-only `torch` and
  `torchvision`, `numpy`, `scipy`, `pillow`, `einops`, `tqdm`, `imageio`,
  `ffmpeg-python`, and `huggingface_hub`. The last is imported by the upstream
  package initializer; no ML model is used or downloaded.

Optional: `CARGO_TARGET_DIR` for the build directory, `CVVDP_RESULTS` and
`CVVDP_BENCHMARK_RESULTS` to move the outputs (default `parity/results/`,
where generated buffers are ignored), `RAYON_NUM_THREADS` (default 16), and
`CVVDP_REPEATS` (default 3).

## Running

From the repository root:

```sh
export CVVDP_REFERENCE=path/to/ColorVideoVDP CVVDP_PYTHON=path/to/python
parity/run.sh
parity/sweep.sh
parity/benchmark.sh
"$CVVDP_PYTHON" parity/report.py parity parity/results/corpus --output parity/MEASURED_PARITY.md
"$CVVDP_PYTHON" parity/report.py benchmark parity/results/benchmark --output parity/MEASURED_PERFORMANCE.md
"$CVVDP_PYTHON" parity/report.py record parity/results/corpus parity/results/benchmark --output parity/MEASURED_RESULTS.json
```

`run.sh` generates the corpus with the reference, then compares it with a
release build without default features and a release build with Rayon. A
comparison fails on missing or malformed files, non-finite values, shape
mismatches, a JOD error above 0.01, a raw map error above 0.001, an internal
f32 map error above 0.0002, feature error above 0.001, or band-frequency error above 0.0001. `report.py`
also requires full corpus coverage and identical single-thread and parallel
results by comparing every serialized output byte, including dimensions,
frequencies, ordered features and maps. `measure.py` captures SHA-256 identities
for Rust sources, lockfiles, build configuration, executable, inputs and outputs,
plus compiler and hardware metadata before execution; it rejects changes during
the run. Reports read that captured metadata and verify artifact hashes, so old
results cannot be relabelled as measurements of a changed checkout.
`MEASURED_PERFORMANCE.md` and the benchmark fields in `MEASURED_RESULTS.json`
contain the 2026-10-02 measurements of the current implementation, with Rust
source and executable identities. The JSON's parity fields retain the previous
record: full `record` regeneration was blocked because the available corpus
and benchmark source snapshots differ in `Cargo.toml`. The corpus was not rerun.

## Corpus

Six image distortions and six video distortions on seven displays; the other
19 embedded displays; all 22 colour spaces on a custom display;
small and mixed-parity image sizes; black and clipped inputs; grayscale;
temporal padding at 1 to 120 fps with 2 to 30 frames; and three reference
example images resized to 64 × 64. The random generator is NumPy PCG64 with
seed 20241001. Reference images are read from the checkout and not
redistributed.

Raw files are little-endian f32. Inputs are in frame, height, width, channel
order; maps in frame, height, width order; features in channel, frame, band
order. `manifest.json` records shapes, display configuration, expected JOD,
software versions, and SHA-256 hashes of the upstream sources and data. The
generator checks that the four JSON files in `data/` match the reference byte
for byte. Both the public f16-quantized map (stored as f32) and the internal
f32 map are saved; the latter is captured by wrapping the bound
`process_block_of_frames` method, without changing its computation.

## Benchmarks

`benchmark.sh` times both implementations on the same 1920 × 1080 f32 inputs:
a noisy image and eight flickering frames at 30 fps. Each measurement is the
median of three predictions after one warm-up, with maps off. The benchmark
harness requests a 2 GiB prediction budget for 1080p video; corpus and sweep
comparisons retain the default 1 GiB budget. Timed: input
validation, colour and display transforms, spatial and temporal processing,
feature extraction, and pooling. Not timed: model construction, input
generation, file I/O, builds, and Python import. Python inputs are passed as
Torch tensors so that no input copy is timed. PyTorch runs on CPU under
`torch.inference_mode` with one inter-op thread and an explicit intra-op thread
count (1, then `RAYON_NUM_THREADS`). The Rust single-thread figure uses a build
without the `parallel` feature.

Compare the single-thread columns with each other. The multi-threaded figures
depend on how each implementation uses threads and are reported separately.
These are wall-clock timings on a shared machine, not throughput guarantees.

## Randomized sweep

`parity/sweep.sh` runs 500 cases by default, using NumPy PCG64 seed 20261002:
4–128-pixel dimensions, images and 2–16-frame videos, both temporal paddings,
random embedded and custom displays, all embedded colour encodings, and noise,
blur, tone, shift and flicker on pattern, random and solid content. Custom
photometry spans the supported domain, including very dim and bright displays.
Two tiny videos qualify the 16384 fps endpoint.

The JOD and feature gates match the fixed corpus. The internal f32 map gate is
0.0002. Public f16 maps use `max(0.001, reference_quantization_error + 0.0002)`:
f16 quantization alone can exceed 0.001 outside the fixed corpus. The original
corpus keeps its strict absolute 0.001 public-map gate. No case is skipped or
regenerated on failure.

Optional variables: `CVVDP_SWEEP_CASES`, `CVVDP_SWEEP_SEED`,
`CVVDP_SWEEP_RESULTS`. The summary is written to `parity/results/sweep.md`;
raw generated buffers and measurement records stay in the results directory.
The sweep runs on CPU and is intentionally excluded from CI because it needs
the heavy reference environment.

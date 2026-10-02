# Parallel performance qualification

2026-10-02; colorvideovdp 0.1.1; Ryzen 9 7945HX, 16 cores / 32 logical CPUs.

Initial `uptime` load averages were **32.89 / 13.77 / 10.15**; an unrelated cooker used roughly 23 CPUs. No unrelated job was stopped or reconfigured. Initial measurements under light shared load reproduced the reported slower scaling (321.01 ms image, 3224.11 ms video), but were not used as the quiet baseline.

The baseline qualification below waited for at most one background CPU equivalent in a one-second `/proc/stat` sample before every benchmark process. The optimized samples are the completed interleaved final-code run with background activity recorded, without this additional gate. Each implementation has five single-thread and five 16-worker samples, interleaved by reversing execution order on alternate repeats. Each process warms both workloads once, then records one prediction each. Model construction, builds and input I/O are outside the timer; validation and the complete prediction are inside. The single-thread binary has default features disabled.

This is a shared desktop, not an exclusive host. The gate measures activity immediately before each process; it cannot exclude a new job starting during a prediction. Load averages include earlier benchmark work and decay slowly. Raw uptime, load and background samples, compiler, source identities and executable hashes are in [PARALLEL_PERFORMANCE.json](PARALLEL_PERFORMANCE.json).

## Before/after measurements

| 1080p workload | Before, 1 thread (ms) | After, 1 thread (ms) | Before, 16 workers (ms) | After, 16 workers (ms) |
| --- | ---: | ---: | ---: | ---: |
| Image, noise | 763.69 | 739.09 | 244.25 | 189.26 |
| Video, 8 frames at 30 fps, flicker | 8982.13 | 8448.71 | 2758.79 | 1971.22 |

Background samples at baseline launch: **0.15–0.93 CPU equivalents**; optimized launch: **0.21–2.56**. A further optimized run with the same one-CPU launch gate was stopped after one single-thread process when background work resumed; its incomplete samples are recorded separately and excluded from medians; the already-complete interleaved run and final harness benchmark are retained. Both parallel targets (220 ms image, 2400 ms video) pass; neither single-thread workload regresses by 3%.

The slowdown against the historical 194.53 ms / 2086.01 ms figures is reproduced on the preserved current baseline. Those historical figures have no source identity, and the pre-hardening implementation no longer exists: a causal claim that hardening introduced this slowdown remains unproven. The before/after improvement here is bound to preserved source snapshots and executable hashes.

## Profile and causes

`perf` could not collect even `task-clock` because `perf_event_paranoid=4`; the system wrapper also lacked a tool for the running kernel. GNU `gprofng` clock sampling at 1 ms worked. Profiles cover both workloads, three repeats and warmups. Static libc/libm symbols are partly unresolved, so source-level stage timers provide the wall-time attribution. Instrumentation existed only in disposable source archives. Profile timings are separate from the uninstrumented qualification; load, allocator state and profiler overhead make their absolute values different.

- **Serial allocation and zero initialization:** `Plane::generate` allocates and fills a new plane on the calling thread before Rayon writes it. Temporary timers measured 131.97 ms per image and 1609.32 ms per video in this path, nested inside the pipeline stages. Reusing consumed contrast, blurred mutual-mask and normalized test buffers removes twelve full-plane allocations per non-baseband image band (sixteen for video), along with their zeroing and extra memory traffic. Allocation failures remain typed errors for every remaining prediction allocation; the conservative budget is unchanged.

- **Serial spatial pooling powers:** before optimization, the stage took 36.86 / 39.64 ms for the image and 398.22 / 440.71 ms for video (one / sixteen workers), showing almost no scaling. Powers now run over rows in existing difference storage after map generation. The f64 sum still runs sequentially in exactly the original pixel order. Profiled parallel pooling fell to 10.95 ms / 106.97 ms. No reduction tree or arithmetic reordering was introduced.

- **Repeated external-caller handoffs:** one Rayon scope keeps the prediction in the current pool, allowing successive spatial passes to fork locally. The scope-only exploratory run improved image/video medians to 271.02 / 2898.64 ms. Sampling recorded `sched_yield` at 5.123 s (7.87% of sampled CPU time) before and 3.979 s (6.90%) after; work stealing and epoch management were also visible. These samples include warmups and input setup, so percentages are not pipeline wall-time fractions.

Checks of the other suspects: budget and fallible-reservation checks are outside per-row closures; rows allocate no user buffers. Finite-input and difference validation remain intact and are small compared with masking and allocation. Final JOD pooling is below 0.01 ms in the profiles. A minimum of 16384 pixels per Rayon task made the scope-only experiment slower (301.52 / 3083.89 ms), so the original row granularity and 4096-pixel threshold are retained. Full-resolution rows span whole cache lines; there is no false-sharing finding. Hardware bandwidth counters were unavailable, so bandwidth saturation is not claimed. The reuse change removes measurable serial initialization and memory traffic. Tuning stopped when the targets passed.

## Pipeline stage wall time

Each cell is the median of three separately instrumented predictions after a warmup. Allocation/zeroing is included in each affected stage, not an additional stage to add to the totals. Tiny setup and final-pooling times are shown for completeness.

### Image

| Stage | Before, 1 thread (ms) | Before, 16 workers (ms) | After, 1 thread (ms) | After, 16 workers (ms) |
| --- | ---: | ---: | ---: | ---: |
| Input validation | 3.584 | 3.457 | 3.318 | 3.636 |
| Setup and filters | 0.013 | 0.011 | 0.009 | 0.013 |
| Colour/display conversion | 117.580 | 20.143 | 112.014 | 26.766 |
| Pyramid reduce/expand | 99.363 | 41.560 | 90.297 | 38.802 |
| Background and contrast | 62.565 | 42.409 | 58.652 | 37.937 |
| CSF sensitivity | 155.462 | 26.626 | 138.152 | 23.352 |
| Masking / baseband differences | 448.904 | 110.212 | 325.117 | 59.593 |
| Difference validation | 2.218 | 4.475 | 3.710 | 2.133 |
| Spatial feature pooling | 36.855 | 39.644 | 35.172 | 10.953 |
| Final pooling and validation | 0.001 | 0.001 | 0.001 | 0.001 |

### Video, 8 frames

| Stage | Before, 1 thread (ms) | Before, 16 workers (ms) | After, 1 thread (ms) | After, 16 workers (ms) |
| --- | ---: | ---: | ---: | ---: |
| Input validation | 28.590 | 29.704 | 25.003 | 26.855 |
| Setup and filters | 0.022 | 0.023 | 0.020 | 0.000 |
| Colour/display conversion | 987.383 | 272.148 | 940.240 | 186.697 |
| Temporal filtering | 1155.697 | 288.547 | 1065.725 | 176.843 |
| Pyramid reduce/expand | 1078.015 | 582.876 | 973.981 | 369.019 |
| Background and contrast | 574.318 | 416.014 | 550.217 | 296.122 |
| CSF sensitivity | 1496.620 | 257.827 | 1378.688 | 232.632 |
| Masking / baseband differences | 4479.338 | 1083.395 | 3253.029 | 576.143 |
| Difference validation | 21.895 | 47.216 | 37.004 | 25.091 |
| Spatial feature pooling | 398.222 | 440.713 | 354.291 | 106.966 |
| Final pooling and validation | 0.004 | 0.004 | 0.004 | 0.004 |

## Correctness and gates

Before editing the library, complete predictions were serialized with explicit little-endian f32 bits and dimensions. Separate before/after output directories were compared byte for byte before scratch cleanup: **1308 predictions, zero differing bytes**. Coverage is the 150-case corpus, 500-case sweep, and both 1080p benchmark cases with maps enabled and disabled, in both default-feature parallel and no-default-feature single-thread builds. Comparisons cover JOD, map presence and shape, every map pixel, band frequencies and ordered per-channel features. The JSON retains every output SHA-256 and size, plus both sets of source file identities. The 1300 fresh corpus/sweep predictions also match these saved baseline bytes.

`parity/run.sh` and the full 500-case `parity/sweep.sh` passed. Corpus maxima remain 0.00000286 JOD, 0.00097513 public-map error and 0.00012809 internal-map error. Sweep maxima remain 0.00001383 JOD and 0.00001627 internal-map error; its public-map gate retains the existing quantization allowance. Every reference gate and full parallel equality check passed.

All commands below returned exit code 0:

- `cargo fmt --check`
- `cargo fmt --check --manifest-path parity/Cargo.toml`
- `cargo fmt --check --manifest-path examples/web/Cargo.toml`
- `cargo clippy --all-targets --all-features --locked -- -D warnings`
- `cargo clippy --all-targets --no-default-features --locked -- -D warnings`
- `cargo clippy --all-targets --manifest-path parity/Cargo.toml --locked -- -D warnings`
- `cargo test --locked`
- `cargo test --no-default-features --locked`
- `cargo build --target wasm32-unknown-unknown --no-default-features --locked`
- `cargo clippy --target wasm32-unknown-unknown --manifest-path examples/web/Cargo.toml --locked -- -D warnings`
- `cargo publish --dry-run --allow-dirty --locked`

The final `parity/benchmark.sh` run used five repeats after one warmup for both Rust and PyTorch thread counts. Its grouped measurements are the figures in [MEASURED_PERFORMANCE.md](MEASURED_PERFORMANCE.md), the README and benchmark SVG; they are separate from the interleaved comparison above. Corpus and benchmark source identities now agree in [MEASURED_RESULTS.json](MEASURED_RESULTS.json). Version and all dependent lockfiles are 0.1.1. No API, algorithm, memory limit, typed-error contract or unsafe-code policy changed. No tests were added for this output-preserving performance change. No commit, tag, push or publish was performed. All requested build and scratch directories were removed after recording the evidence.

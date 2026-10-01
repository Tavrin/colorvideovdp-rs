# Randomized parity sweep

PASS: 500 cases; NumPy PCG64 seed 20261002.

Reference: ColorVideoVDP 0.5.7, revision 2a268bc; PyTorch 2.14.1+cpu, CPU, 1 thread(s).

Rust source/configuration SHA-256: `9c412490fde1c6881f0f5a3d284b3f150afdfc2c01b60d1aabf6494c00855443`.
Single-thread binary SHA-256: `221505218f84d539ddf8e9d34a96bc2c9a90ed66ca050b379296c1e7c19d4b17`.
Parallel binary SHA-256: `848cc8da4ac20b7e35f9e216e23b0a5d1b11a946d8f50d18736e01054b8038cd`.

| Corpus | Cases | Max absolute JOD error | Max raw map error | Max map error before f16 |
| --- | ---: | ---: | ---: | ---: |
| random videos | 257 | 0.00001383 | 0.00193739 | 0.00001627 |
| random images | 243 | 0.00000286 | 0.00048232 | 0.00000554 |
| **Total** | **500** | **0.00001383** | **0.00193739** | **0.00001627** |

Maximum absolute feature error: 0.0002670288; band frequency error: 0.

Complete predictions (score, dimensions, channels, band frequencies, ordered features and every map pixel) are bit-identical between single-thread and parallel builds.

Gates: absolute JOD error ≤ 0.01; internal f32 map error ≤ 0.0002; absolute feature error ≤ 0.001; band frequency error ≤ 0.0001. Public f16 maps use max(0.001, measured reference f16 quantization error + 0.0002), since f16 rounding alone can exceed 0.001. The fixed 150-case corpus retains its absolute 0.001 public-map gate.

Widths: 4–128; heights: 4–128; video frames: 2–16; video fps: 1–16384.
Embedded displays exercised: 26; custom displays: 250; colour encodings: 22.
Custom peak luminance (cd/m²): 0.00616949–790411; contrast: 1.00773–9.42974e+06; ambient illuminance (lux): 0.000107868–93963.2; exposure: 0.0104914–95.859.

Distortions: blur 86, flicker 110, noise 112, shift 93, tone 99.
Contents: noise 160, pattern 178, solid 162.

Reproduce with `parity/sweep.sh`; set `CVVDP_REFERENCE` and `CVVDP_PYTHON` as described in the parity README. Per-file source, input, output, compiler, configuration and hardware identities are captured at execution time in the measurement JSON files. No GPU is used.

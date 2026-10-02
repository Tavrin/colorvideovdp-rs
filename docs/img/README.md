# Images

The images in this directory are used by the top-level README. They are not
part of the published crate.

| File | Content | Source |
|---|---|---|
| `hero.webp` | Reference, test and distortion map of a procedural scene, with the JOD score | `examples/showcase.rs` |
| `jod-levels.webp` | The same scene with noise, blur, blocking and a colour shift at three strengths, each with its JOD score | `examples/showcase.rs` |
| `video-strip.webp` | Six frames of a moving scene, a flickering test sequence and the per-frame distortion map | `examples/showcase.rs` |
| `benchmarks.svg` | Timings from `parity/MEASURED_PERFORMANCE.md` | `generate.py` |

All scenes are procedural and were made for this repository. They contain no
third-party images and are covered by the repository's MIT licence. The demo's
sample pair, `examples/web/sample-reference.png` and `sample-test.png`, is the
hero pair.

The reference implementation ships example media (`example_media/`, including
`tree.jpg`, `wavy_facade.png` and several videos). Its repository is under the
MIT licence, but it does not state where those images and videos come from or
whether the licence covers them, so they are not reproduced here.

Every score and map is computed by this crate on the `standard_fhd` display
(24 inches, 1920 × 1080, 200 cd/m², viewed from 0.6 m), with images
quantized to 8-bit sRGB before the comparison. Map values are
`1 - per_pixel_JOD / 10`, shown from 0 (black) to 0.5 (per-pixel JOD 5, light
yellow) with a ramp that approximates the Magma colour map; larger values are
clipped. The WebP figures are lossy (quality 90), so the pixels are close to,
but not exactly, the crate's output.

The benchmark chart uses the 2026-10-02 measurements of the current
implementation after hardening, from `parity/MEASURED_PERFORMANCE.md`. Rust
source and executable identities are stored in `parity/MEASURED_RESULTS.json`.

## Regenerating

From the repository root, with cargo, NumPy and Pillow (built with WebP
support):

```sh
python3 docs/img/generate.py
```

This rewrites the four figures and the demo's sample pair. The output is
deterministic for a given toolchain and Pillow version.

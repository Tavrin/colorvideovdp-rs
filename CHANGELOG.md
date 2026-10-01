# Changelog

All notable changes to this crate are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
crate follows [Semantic Versioning](https://semver.org/).

## [0.1.0] - Unreleased

### Added

- Checked byte layouts, fallible prediction buffers and a configurable 1 GiB
  default memory budget, with explicit errors.
- Indexed temporal caches and inverse DFT evaluation that omits exactly zero
  response terms, including qualification at 16384 fps.
- A documented numerical photometry domain; unsupported extremes return
  `InvalidDisplay` instead of diverging silently from reference reductions.
- Non-exhaustive photometry/display records with validated constructors.
- A 500-case randomized parity sweep, full-output parallel equality, feature
  gates, execution-time source/binary provenance and three fuzz targets.
- Packaged image example and repository links for development assets.

- ColorVideoVDP base model v0.5.7 for f32 images and frame sequences:
  `Cvvdp::predict_image` and `Cvvdp::predict_video` return JOD, optional raw
  per-pixel distortion maps, and pooled per-channel features.
- The reference display models, colour spaces, model parameters, and castleCSF
  lookup table, embedded unchanged.
- Custom displays from `Photometry` and `Geometry`, or from JSON in the
  reference `display_models.json` format.
- Replicate and symmetric temporal padding.
- Single-threaded execution, and Rayon row parallelism behind the default
  `parallel` feature, with identical results.
- Builds for `wasm32-unknown-unknown` without default features.
- Tests against three fixtures from the reference implementation, and an
  unpublished harness that checks 150 cases against reference revision
  `2a268bc` (maximum JOD error 0.00000286).

[0.1.0]: https://github.com/Tavrin/colorvideovdp-rs/releases/tag/v0.1.0

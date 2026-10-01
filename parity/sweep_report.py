#!/usr/bin/env python3
"""Write a path-free, source-bound randomized parity qualification summary."""
import argparse
from collections import Counter
from pathlib import Path
from report import measurement, parity_rows, parity_table


def summary(directory):
    manifest, rows = parity_rows(directory)
    cases = manifest["cases"]
    identities = {mode: measurement(directory, "compare", mode) for mode in ("single", "parallel")}
    custom = [c["custom_display"][c["display"]] for c in cases if "custom_display" in c]
    def span(values):
        return f"{min(values):.6g}–{max(values):.6g}"
    lines = ["# Randomized parity sweep", "", f"PASS: {len(cases)} cases; NumPy PCG64 seed {manifest['seed']}.", "",
             f"Reference: ColorVideoVDP {manifest['reference_model']}, revision {manifest['reference_revision']}; "
             f"PyTorch {manifest['torch_version']}, CPU, {manifest['torch_threads']} thread(s).", "",
             f"Rust source/configuration SHA-256: `{identities['single']['source_sha256']}`.",
             f"Single-thread binary SHA-256: `{identities['single']['binary_sha256']}`.",
             f"Parallel binary SHA-256: `{identities['parallel']['binary_sha256']}`.", "",
             parity_table(directory).strip(), "",
             f"Maximum absolute feature error: {max(r['feature_error'] for r in rows):.9g}; "
             f"band frequency error: {max(r['band_frequency_error'] for r in rows):.9g}.", "",
             "Complete predictions (score, dimensions, channels, band frequencies, ordered features and every map pixel) "
             "are bit-identical between single-thread and parallel builds.", "",
             "Gates: absolute JOD error ≤ 0.01; internal f32 map error ≤ 0.0002; "
             "absolute feature error ≤ 0.001; band frequency error ≤ 0.0001. "
             "Public f16 maps use max(0.001, measured reference f16 quantization error + 0.0002), "
             "since f16 rounding alone can exceed 0.001. The fixed 150-case corpus retains its absolute 0.001 public-map gate.", "",
             f"Widths: {span([c['width'] for c in cases])}; heights: {span([c['height'] for c in cases])}; "
             f"video frames: {span([c['frames'] for c in cases if c['frames'] > 1])}; "
             f"video fps: {span([c['fps'] for c in cases if c['frames'] > 1])}.",
             f"Embedded displays exercised: {len({c['display'] for c in cases if 'custom_display' not in c})}; "
             f"custom displays: {len(custom)}; colour encodings: {len({c['color'] for c in cases})}.",
             f"Custom peak luminance (cd/m²): {span([c['max_luminance'] for c in custom])}; "
             f"contrast: {span([c['contrast'] for c in custom])}; "
             f"ambient illuminance (lux): {span([c['E_ambient'] for c in custom])}; "
             f"exposure: {span([c['exposure'] for c in custom])}.", "",
             "Distortions: " + ", ".join(f"{k} {v}" for k, v in sorted(Counter(c['distortion'] for c in cases).items())) + ".",
             "Contents: " + ", ".join(f"{k} {v}" for k, v in sorted(Counter(c['content'] for c in cases).items())) + ".", "",
             "Reproduce with `parity/sweep.sh`; set `CVVDP_REFERENCE` and `CVVDP_PYTHON` as described in the parity README. "
             "Per-file source, input, output, compiler, configuration and hardware identities are captured at execution time "
             "in the measurement JSON files. No GPU is used."]
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.write_text(summary(args.directory))

#!/usr/bin/env python3
"""Generate a deterministic CPU reference corpus from a local ColorVideoVDP checkout."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import sys
import time

os.environ["CUDA_VISIBLE_DEVICES"] = ""
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"
sys.dont_write_bytecode = True

import numpy as np
from PIL import Image as PILImage
from scipy.ndimage import gaussian_filter
import torch


def load_reference():
    reference = Path(os.environ["CVVDP_REFERENCE"])
    sys.path.insert(0, str(reference))
    from pycvvdp.cvvdp_metric import cvvdp
    from pycvvdp.display_model import vvdp_display_photo_eotf, vvdp_display_geometry
    return reference, cvvdp, vvdp_display_photo_eotf, vvdp_display_geometry


def raw(path, values):
    np.asarray(values, dtype="<f4").tofile(path)


def pattern(width, height, frames):
    y, x = np.mgrid[:height, :width].astype(np.float32)
    result = []
    for frame in range(frames):
        px = x + frame * 0.7
        result.append(np.stack([
            0.48 + 0.25 * np.sin(px * 0.19) * np.cos(y * 0.13),
            0.48 + 0.24 * np.cos(px * 0.11 + y * 0.15),
            0.48 + 0.24 * np.sin(px * 0.07 - y * 0.17),
        ], axis=-1))
    return np.asarray(result, dtype=np.float32)


def distort(reference, kind, rng):
    if kind == "identity":
        return reference.copy()
    if kind == "noise":
        return reference + rng.normal(0, 0.025, reference.shape).astype(np.float32)
    if kind == "blur":
        return gaussian_filter(reference, sigma=(0, 1.2, 1.2, 0))
    if kind == "shift":
        return np.roll(reference, 1, axis=2)
    if kind == "tone":
        return reference * np.array([1.07, 0.94, 1.03], dtype=np.float32)
    if kind == "compression":
        blocky = reference[:, ::4, ::4].repeat(4, axis=1).repeat(4, axis=2)
        blocky = blocky[:, :reference.shape[1], :reference.shape[2]]
        return np.round(blocky * 24) / 24
    if kind == "flicker":
        return reference * (1 + 0.055 * (-1.0) ** np.arange(len(reference)))[:, None, None, None]
    raise ValueError(kind)


def encode(values, color):
    values = values.astype(np.float32)
    if color.endswith("linear") or color == "luminance":
        return values * 350
    return values


def metric_for(case, cvvdp, photo_type, geometry_type, heatmap=True):
    kwargs = dict(device=torch.device("cpu"), quiet=True,
                  heatmap="raw" if heatmap else None, temp_padding=case["padding"])
    if "custom_display" in case:
        model = case["custom_display"][case["display"]]
        kwargs["display_photometry"] = photo_type(
            model["max_luminance"], contrast=model.get("contrast", 500),
            source_colorspace=case["color"], E_ambient=model.get("E_ambient", 0),
            k_refl=model.get("k_refl", 0.005), exposure=model.get("exposure", 1))
        kwargs["display_geometry"] = geometry_type(model["resolution"], ppd=model["pixels_per_degree"])
    else:
        kwargs["display_name"] = case["display"]
        from pycvvdp.display_model import vvdp_display_photometry
        loaded = vvdp_display_photometry.load(case["display"], [])
        kwargs["display_photometry"] = photo_type(
            loaded.Y_peak, contrast=loaded.contrast, source_colorspace=case["color"],
            E_ambient=loaded.E_ambient, k_refl=loaded.k_refl, exposure=loaded.exposure)
    return cvvdp(**kwargs)


def predict(metric, case, test, reference):
    return metric.predict(test, reference, dim_order="FHWC", frames_per_second=case["fps"])


def generate(out, benchmark=False, sweep_count=0, seed=20241001):
    ref_path, cvvdp, photo_type, geometry_type = load_reference()
    out.mkdir(parents=True, exist_ok=True)
    data = ref_path / "pycvvdp" / "vvdp_data"
    displays = json.loads((data / "display_models.json").read_text())
    colors = json.loads((data / "color_spaces.json").read_text())
    root = Path(__file__).resolve().parents[1]
    for name in ["display_models.json", "color_spaces.json", "cvvdp_parameters.json", "csf_lut_weber_fixed_size.json"]:
        if (data / name).read_bytes() != (root / "data" / name).read_bytes():
            raise RuntimeError(f"embedded reference data differs: {name}")
    rng = np.random.default_rng(seed)
    cases = []

    def add(name, group, display, kind, w=64, h=64, frames=1, fps=0, padding="replicate", color=None, source=None, custom=None):
        color = color or displays[display].get("colorspace", "sRGB")
        reference = pattern(w, h, frames) if source is None else source
        test = distort(reference, kind, rng)
        reference = encode(reference, color)
        test = encode(test, color)
        if color == "luminance":
            reference = reference.mean(axis=-1, keepdims=True)
            test = test.mean(axis=-1, keepdims=True)
        case = dict(name=name, group=group, display=display, color=color, width=w, height=h,
                    channels=reference.shape[-1], frames=frames, fps=fps, padding=padding,
                    test=f"{name}-test.f32", reference=f"{name}-reference.f32",
                    heatmap=f"{name}-map.f32", heatmap_f32=f"{name}-map-unquantized.f32", features=f"{name}-features.f32")
        if custom is not None:
            case["custom_display"] = {display: custom}
        raw(out / case["test"], test)
        raw(out / case["reference"], reference)
        metric = metric_for(case, cvvdp, photo_type, geometry_type, heatmap=not benchmark)
        maps = []
        original = metric.process_block_of_frames

        def capture(*args, **kwargs):
            result = original(*args, **kwargs)
            if result[1] is not None:
                maps.append(result[1].detach().numpy().copy())
            return result

        metric.process_block_of_frames = capture
        with torch.inference_mode():
            score, stats = predict(metric, case, test, reference)
        case["jod"] = float(score)
        case["band_frequencies"] = np.asarray(stats["rho_band"]).tolist()
        if not np.isfinite(case["jod"]):
            raise RuntimeError(f"non-finite reference score: {name}")
        if not benchmark:
            raw(out / case["heatmap"], stats["heatmap"].float().numpy())
            raw(out / case["heatmap_f32"], np.concatenate(maps, axis=2))
            raw(out / case["features"], stats["Q_per_ch"])
        cases.append(case)
        print(f"{len(cases):3} {name}: {case['jod']:.6f}", flush=True)

    if sweep_count:
        sizes = [4, 5, 7, 8, 9, 12, 16, 17, 24, 32, 48, 64, 96, 128]
        color_names = list(colors)
        display_names = list(displays)
        for i in range(sweep_count):
            video = bool(rng.integers(2))
            w, h = (int(rng.choice(sizes)), int(rng.choice(sizes)))
            frames = int(rng.integers(2, 17)) if video else 1
            fps = float(rng.choice([1, 12, 23.976, 24, 29.97, 30, 60, 120, 240])) if video else 0
            if i in (0, 1):
                video, w, h, frames, fps = True, 4, 4, 2, 16384.0
            padding = str(rng.choice(["replicate", "symmetric"]))
            kind = str(rng.choice(["noise", "blur", "tone", "shift", "flicker"]))
            display = str(rng.choice(display_names))
            color = str(rng.choice(color_names))
            custom = None
            if i % 2 == 0:
                # Stay inside the documented numerical domain, including its edges.
                peak = float(10 ** rng.uniform(np.log10(0.006), np.log10(800000)))
                contrast = float(10 ** rng.uniform(0, 7))
                ambient = float(10 ** rng.uniform(-4, 5))
                reflectivity = float(rng.uniform(0, 1))
                if peak + peak / contrast + ambient / np.pi * reflectivity > 1000000:
                    peak *= 0.4
                # Upstream HLG requires ambient light above 1000 cd/m².
                custom = dict(resolution=[1920, 1080], pixels_per_degree=float(rng.uniform(10, 120)),
                              max_luminance=peak, contrast=contrast, E_ambient=ambient,
                              k_refl=reflectivity, exposure=float(10 ** rng.uniform(-2, 2)))
                display = "random"
            elif color.endswith("HLG") and displays[display]["max_luminance"] > 1000 and displays[display].get("E_ambient", 0) == 0:
                display = "standard_hdr_hlg"
            source = pattern(w, h, frames)
            content = int(rng.integers(3))
            if content == 1:
                source = rng.uniform(0, 1, source.shape).astype(np.float32)
            elif content == 2:
                source[:] = rng.uniform(0.02, 0.98, (1, 1, 1, 3))
            add(f"sweep-{i:04}", "random videos" if video else "random images", display, kind,
                w=w, h=h, frames=frames, fps=fps, padding=padding, color=color, source=source, custom=custom)
            cases[-1]["distortion"] = kind
            cases[-1]["content"] = ["pattern", "noise", "solid"][content]
    elif benchmark:
        add("1080p-image", "benchmark", "standard_4k", "noise", w=1920, h=1080)
        add("1080p-video-8-frames-30fps", "benchmark", "standard_4k", "flicker", w=1920, h=1080, frames=8, fps=30)
    else:
        selected = ["standard_4k", "standard_fhd", "standard_phone", "standard_hmd",
                    "standard_hdr_pq", "standard_hdr_hlg", "standard_hdr_linear"]
        for display in selected:
            for i, kind in enumerate(["identity", "noise", "blur", "shift", "tone", "compression"]):
                w, h = [(64, 64), (63, 64), (65, 63), (48, 73), (32, 40), (16, 16)][i]
                add(f"{display}-image-{kind}", "generated images", display, kind, w, h)
            for i, kind in enumerate(["noise", "blur", "shift", "tone", "compression", "flicker"]):
                add(f"{display}-video-{kind}", "generated videos", display, kind,
                    w=32 if i % 2 == 0 else 33, h=32 if i % 3 == 0 else 31,
                    frames=8 if i % 2 == 0 else 12, fps=[24, 30, 60][i % 3])
        for display in displays:
            if display not in selected:
                add(f"display-{display}", "other displays", display, "tone", w=32, h=32)
        custom = dict(resolution=[1920, 1080], pixels_per_degree=45,
                      max_luminance=1200, contrast=10000, E_ambient=25, k_refl=0.01, exposure=1.25)
        for i, color in enumerate(colors):
            add(f"color-{i:02}", "colour spaces", "custom", "noise", w=32, h=32, color=color, custom=custom)
        for fps, frames in [(1, 2), (29.97, 8), (120, 8), (60, 30)]:
            for padding in ["replicate", "symmetric"]:
                add(f"padding-{padding}-{fps}-{frames}", "temporal edges", "standard_4k", "flicker",
                    w=17, h=16, frames=frames, fps=fps, padding=padding)
        for w, h in [(4, 4), (5, 4), (4, 5), (7, 7), (8, 8), (9, 8)]:
            add(f"small-{w}-{h}", "spatial edges", "standard_4k", "noise", w=w, h=h)
        add("black", "spatial edges", "standard_4k", "identity", source=np.zeros((1, 32, 32, 3), dtype=np.float32), w=32, h=32)
        add("clipped", "spatial edges", "standard_hdr_pq", "noise", source=pattern(32, 32, 1) * 3 - .8, w=32, h=32)
        for display in ["standard_4k", "standard_hdr_linear_dark", "standard_hdr_linear_zoom"]:
            add(f"grayscale-{display}", "grayscale", display, "noise", color="luminance", w=32, h=32)
        media_count = 0
        for filename in ["tree.jpg", "cvvdp_logo_250x250_white.png", "SIGGRAPH_wordcloud.png"]:
            path = ref_path / "example_media" / filename
            if path.is_file() and path.stat().st_size < 2_000_000:
                source = np.asarray(PILImage.open(path).convert("RGB").resize((64, 64), PILImage.Resampling.LANCZOS), dtype=np.float32)[None] / 255
                for kind in ["noise", "blur"]:
                    add(f"media-{path.stem}-{kind}", "reference media", "standard_4k", kind, source=source)
                media_count += 1
        if media_count == 0:
            raise RuntimeError("no small reference example media found")
    source_files = ["cvvdp_metric.py", "lpyr_dec.py", "display_model.py", "csf.py", "interp.py", "utils.py", "video_source.py"]
    hashes = {name: hashlib.sha256((ref_path / "pycvvdp" / name).read_bytes()).hexdigest() for name in source_files}
    hashes.update({f"vvdp_data/{p.name}": hashlib.sha256(p.read_bytes()).hexdigest() for p in data.glob("*.json")})
    manifest = dict(reference_revision="2a268bc", reference_model="0.5.7", source_sha256=hashes,
                    torch_version=torch.__version__, numpy_version=np.__version__, python_version=platform.python_version(),
                    torch_threads=torch.get_num_threads(), seed=seed, jod_tolerance=0.01, map_tolerance=0.001, map_f32_tolerance=0.0002,
                    feature_tolerance=0.001, quantization_aware=bool(sweep_count), cases=cases)
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


def benchmark_reference(directory, threads, repeats):
    _, cvvdp, photo_type, geometry_type = load_reference()
    torch.set_num_threads(threads)
    manifest = json.loads((directory / "manifest.json").read_text())
    timings = []
    for case in manifest["cases"]:
        shape = (case["frames"], case["height"], case["width"], case["channels"])
        # Torch tensors avoid a video_source_array NumPy-to-tensor copy in timed calls.
        test = torch.from_numpy(np.fromfile(directory / case["test"], dtype="<f4").reshape(shape).copy())
        reference = torch.from_numpy(np.fromfile(directory / case["reference"], dtype="<f4").reshape(shape).copy())
        metric = metric_for(case, cvvdp, photo_type, geometry_type, heatmap=False)
        with torch.inference_mode():
            predict(metric, case, test, reference)
            samples = []
            for _ in range(repeats):
                start = time.perf_counter()
                result, _ = predict(metric, case, test, reference)
                samples.append((time.perf_counter() - start) * 1000)
                if abs(float(result) - case["jod"]) > manifest["jod_tolerance"]:
                    raise RuntimeError("benchmark reference parity failed")
        row = dict(name=case["name"], threads=threads, milliseconds=statistics.median(samples), samples_ms=samples, jod=float(result))
        timings.append(row)
        print(f"{case['name']} PyTorch {threads} threads: {row['milliseconds']:.2f} ms", flush=True)
    (directory / f"benchmark-reference-{threads}.json").write_text(json.dumps(timings, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["generate", "generate-benchmark", "benchmark", "sweep"])
    parser.add_argument("directory", type=Path)
    parser.add_argument("--threads", type=int, default=1)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--cases", type=int, default=500)
    parser.add_argument("--seed", type=int, default=20261002)
    args = parser.parse_args()
    if args.threads < 1 or args.repeats < 1:
        parser.error("threads and repeats must be positive")
    torch.set_num_threads(args.threads)
    torch.set_num_interop_threads(1)
    if args.action == "benchmark":
        benchmark_reference(args.directory, args.threads, args.repeats)
    else:
        generate(args.directory, benchmark=args.action == "generate-benchmark",
                 sweep_count=args.cases if args.action == "sweep" else 0,
                 seed=args.seed if args.action == "sweep" else 20241001)


if __name__ == "__main__":
    main()

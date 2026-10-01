#!/usr/bin/env python3
"""Summarize parity and benchmark results as Markdown tables and a JSON record.

    report.py parity CORPUS_DIR [--output FILE]
    report.py benchmark BENCHMARK_DIR [--output FILE]
    report.py record CORPUS_DIR BENCHMARK_DIR --output FILE

Run `record` on the machine that produced the benchmark results.
"""
import argparse
from collections import defaultdict
import hashlib
import math
import json
import os
from pathlib import Path
import platform

WORKLOADS = {
    "1080p-image": "Image, noise",
    "1080p-video-8-frames-30fps": "Video, 8 frames at 30 fps, flicker",
}


def load(path):
    return json.loads(Path(path).read_text())


def measurement(directory, action, mode):
    record = load(directory / f"measurement-{action}-{mode}.json")
    for group in ("input_sha256", "output_sha256"):
        for name, expected in record[group].items():
            if hashlib.sha256((directory / name).read_bytes()).hexdigest() != expected:
                raise RuntimeError(f"measurement artifact changed: {name}")
    if not record.get("source_sha256") or not record.get("binary_sha256"):
        raise RuntimeError("measurement lacks source/binary identity")
    return record


def parity_rows(directory):
    manifest = load(directory / "manifest.json")
    single = load(directory / "comparison-single.json")
    parallel = load(directory / "comparison-parallel.json")
    identities = [measurement(directory, "compare", mode) for mode in ("single", "parallel")]
    if identities[0]["source_sha256"] != identities[1]["source_sha256"]:
        raise RuntimeError("single and parallel measured different sources")
    expected = {c["name"] for c in manifest["cases"]}
    if not expected or {r["name"] for r in single} != expected or {r["name"] for r in parallel} != expected:
        raise RuntimeError("comparison corpus is incomplete")
    if len(single) != len(expected) or len(parallel) != len(expected) or len(manifest["cases"]) != len(expected):
        raise RuntimeError("duplicate parity cases")
    for name in expected:
        if (directory / f"{name}-prediction-single.bin").read_bytes() != (directory / f"{name}-prediction-parallel.bin").read_bytes():
            raise RuntimeError(f"full parallel prediction differs: {name}")
    if single != parallel:
        raise RuntimeError("single-thread and parallel predictions differ")
    for row in single:
        map_limit = manifest["map_tolerance"]
        if manifest.get("quantization_aware"):
            map_limit = max(map_limit, row["reference_quantization_error"] + manifest["map_f32_tolerance"])
        if (any(not math.isfinite(v) for v in row.values() if isinstance(v, (int, float)))
            or row["jod_error"] > manifest["jod_tolerance"] or row["map_error"] > map_limit
            or row["map_f32_error"] > manifest["map_f32_tolerance"]
            or row["feature_error"] > manifest["feature_tolerance"] or row["band_frequency_error"] > 0.0001):
            raise RuntimeError(f"parity target failed: {row['name']}")
    return manifest, single


def parity_table(directory):
    _, rows = parity_rows(directory)
    groups = defaultdict(list)
    for row in rows:
        groups[row["group"]].append(row)
    lines = ["| Corpus | Cases | Max absolute JOD error | Max raw map error | Max map error before f16 |",
             "| --- | ---: | ---: | ---: | ---: |"]
    for name, group in groups.items():
        lines.append(f"| {name} | {len(group)} | {max(r['jod_error'] for r in group):.8f} | "
                     f"{max(r['map_error'] for r in group):.8f} | {max(r['map_f32_error'] for r in group):.8f} |")
    lines.append(f"| **Total** | **{len(rows)}** | **{max(r['jod_error'] for r in rows):.8f}** | "
                 f"**{max(r['map_error'] for r in rows):.8f}** | **{max(r['map_f32_error'] for r in rows):.8f}** |")
    return "\n".join(lines) + "\n"


def benchmark_files(directory):
    references = sorted(directory.glob("benchmark-reference-*.json"))
    threaded = [p for p in references if p.stem != "benchmark-reference-1"]
    if len(threaded) != 1:
        raise RuntimeError("expected one multi-threaded reference benchmark")
    return {
        "rust_single": directory / "benchmark-rust-SingleThread.json",
        "reference_single": directory / "benchmark-reference-1.json",
        "rust_parallel": directory / "benchmark-rust-Parallel.json",
        "reference_parallel": threaded[0],
    }


def medians(path):
    return {row["name"]: row["milliseconds"] for row in load(path)}


def benchmark_table(directory):
    identities = [measurement(directory, "benchmark", mode) for mode in ("single", "parallel")]
    if identities[0]["source_sha256"] != identities[1]["source_sha256"]:
        raise RuntimeError("benchmark modes measured different sources")
    files = benchmark_files(directory)
    timings = {key: medians(path) for key, path in files.items()}
    threads = load(files["reference_parallel"])[0]["threads"]
    lines = ["Single thread:", "",
             "| 1080p workload | Rust (ms) | PyTorch CPU (ms) | PyTorch / Rust |",
             "| --- | ---: | ---: | ---: |"]
    for name, label in WORKLOADS.items():
        rust, reference = timings["rust_single"][name], timings["reference_single"][name]
        lines.append(f"| {label} | {rust:.2f} | {reference:.2f} | {reference / rust:.1f}× |")
    lines += ["", f"Multi-threaded ({threads} Rayon workers; {threads} PyTorch intra-op threads):", "",
              "| 1080p workload | Rust (ms) | PyTorch CPU (ms) |",
              "| --- | ---: | ---: |"]
    for name, label in WORKLOADS.items():
        lines.append(f"| {label} | {timings['rust_parallel'][name]:.2f} | {timings['reference_parallel'][name]:.2f} |")
    return "\n".join(lines) + "\n"


def hardware():
    info = {"cpu": platform.processor() or platform.machine(), "logical_cpus": os.cpu_count(),
            "os": f"{platform.system()} {platform.release()} {platform.machine()}"}
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.is_file():
        cores, physical = set(), None
        for line in cpuinfo.read_text().splitlines():
            key, _, value = (part.strip() for part in line.partition(":"))
            if key == "model name":
                info["cpu"] = value
            elif key == "physical id":
                physical = value
            elif key == "core id":
                cores.add((physical, value))
        if cores:
            info["physical_cores"] = len(cores)
    meminfo = Path("/proc/meminfo")
    if meminfo.is_file():
        for line in meminfo.read_text().splitlines():
            if line.startswith("MemTotal:"):
                info["ram_gib"] = int(line.split()[1]) // (1024 * 1024)
    return info


def record(corpus, bench):
    manifest, rows = parity_rows(corpus)
    files = benchmark_files(bench)
    samples = {path.stem: load(path) for path in files.values()}
    identities = {f"{action}_{mode}": measurement(directory, action, mode)
                  for action, directory in (("compare", corpus), ("benchmark", bench)) for mode in ("single", "parallel")}
    if len({item["source_sha256"] for item in identities.values()}) != 1:
        raise RuntimeError("measurements used different source snapshots")
    return {
        "rust_measurements": identities,
        "hardware": identities["benchmark_single"]["hardware"],
        "compiler": identities["benchmark_single"]["compiler"],
        "profile": "release",
        "heatmaps_in_benchmarks": False,
        "reference_revision": manifest["reference_revision"],
        "model_version": manifest["reference_model"],
        "torch": manifest["torch_version"],
        "numpy": manifest["numpy_version"],
        "python": manifest["python_version"],
        "reference_source_sha256": manifest["source_sha256"],
        "seed": manifest["seed"],
        "jod_tolerance": manifest["jod_tolerance"],
        "map_tolerance": manifest["map_tolerance"],
        "parity_cases": rows,
        "parallel_predictions_identical": True,
        "benchmark_warmups": 1,
        "benchmark_repetitions": len(samples["benchmark-reference-1"][0]["samples_ms"]),
        "benchmark_samples": samples,
        "benchmark_case_scores": {c["name"]: c["jod"] for c in load(bench / "manifest.json")["cases"]},
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["parity", "benchmark", "record"])
    parser.add_argument("directories", type=Path, nargs="+")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.action == "record":
        if len(args.directories) != 2 or args.output is None:
            parser.error("record needs CORPUS_DIR, BENCHMARK_DIR, and --output")
        output = json.dumps(record(*args.directories), indent=2) + "\n"
    elif len(args.directories) != 1:
        parser.error(f"{args.action} needs one directory")
    elif args.action == "parity":
        output = parity_table(args.directories[0])
    else:
        output = benchmark_table(args.directories[0])
    if args.output:
        args.output.write_text(output)
    print(output, end="")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Build, identify, execute and bind a Rust measurement to immutable content."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
from report import hardware

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_identity():
    paths = [ROOT / name for name in ("Cargo.toml", "Cargo.lock", "parity/Cargo.toml", "parity/Cargo.lock")]
    for folder, pattern in (("src", "*.rs"), ("data", "*.json"), ("parity/src", "*.rs")):
        paths += sorted((ROOT / folder).rglob(pattern))
    paths += sorted((ROOT / "parity").glob("*.py")) + sorted((ROOT / "parity").glob("*.sh"))
    sources = {p.relative_to(ROOT).as_posix(): digest(p) for p in paths}
    snapshot = hashlib.sha256(json.dumps(sources, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return dict(source_sha256=snapshot, files_sha256=sources)


def build_configuration():
    environment = {k: os.environ.get(k) for k in (
        "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_TARGET", "CARGO_BUILD_RUSTFLAGS",
        "CARGO_PROFILE_RELEASE_OPT_LEVEL", "CARGO_PROFILE_RELEASE_LTO", "CARGO_PROFILE_RELEASE_CODEGEN_UNITS",
        "RAYON_NUM_THREADS", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER")}
    configs = {}
    for level, parent in enumerate([ROOT, *ROOT.parents]):
        for name in ("config", "config.toml"):
            path = parent / ".cargo" / name
            if path.is_file():
                configs[f"ancestor-{level}/{name}"] = digest(path)
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    for name in ("config", "config.toml"):
        if (cargo_home / name).is_file():
            configs[f"cargo-home/{name}"] = digest(cargo_home / name)
    return dict(environment=environment, cargo_config_sha256=configs)


def run(directory, mode, action, repeats):
    compiler = subprocess.check_output(["rustc", "-vV"], text=True).strip()
    before = source_identity()
    configuration = build_configuration()
    command = ["cargo", "build", "--locked", "--release", "--manifest-path", "parity/Cargo.toml"]
    if mode == "single":
        command.append("--no-default-features")
    subprocess.run(command, cwd=ROOT, check=True)
    binary = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "release" / "colorvideovdp-parity"
    binary_hash = digest(binary)
    if source_identity() != before or build_configuration() != configuration:
        raise RuntimeError("source changed during build")
    # Bind input/reference buffers as well as the manifest; reports verify these
    # later without consulting a changed checkout or current compiler/hardware.
    manifest = json.loads((directory / "manifest.json").read_text())
    input_paths = {"manifest.json"}
    for case in manifest["cases"]:
        input_paths.update(case[k] for k in ("test", "reference", "heatmap", "heatmap_f32", "features") if (directory / case[k]).is_file())
    inputs = {name: digest(directory / name) for name in sorted(input_paths)}
    record = dict(**before, binary_sha256=binary_hash, compiler=compiler,
                  cargo=subprocess.check_output(["cargo", "-V"], text=True).strip(),
                  hardware=hardware(), build_command=command, build_environment=configuration["environment"],
                  cargo_config_sha256=configuration["cargo_config_sha256"],
                  execution_threads=1 if mode == "single" else int(os.environ.get("RAYON_NUM_THREADS", "16")), profile="release", features=[] if mode == "single" else ["parallel"],
                  execution=mode, action=action, input_sha256=inputs,
                  started_utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()))
    subprocess.run([str(binary), action, str(directory), mode, str(repeats)], check=True, cwd=ROOT)
    if source_identity() != before or build_configuration() != configuration or digest(binary) != binary_hash or subprocess.check_output(["rustc", "-vV"], text=True).strip() != compiler:
        raise RuntimeError("source, binary or compiler changed during measurement")
    if any(digest(directory / name) != value for name, value in inputs.items()):
        raise RuntimeError("corpus changed during measurement")
    if action == "compare":
        outputs = [directory / f"comparison-{mode}.json", *sorted(directory.glob(f"*-prediction-{mode}.bin"))]
    else:
        outputs = [directory / f"benchmark-rust-{'SingleThread' if mode == 'single' else 'Parallel'}.json"]
    record["output_sha256"] = {p.name: digest(p) for p in outputs}
    (directory / f"measurement-{action}-{mode}.json").write_text(json.dumps(record, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("mode", choices=["single", "parallel"])
    parser.add_argument("action", choices=["compare", "benchmark"])
    parser.add_argument("--repeats", type=int, default=3)
    args = parser.parse_args()
    run(args.directory.resolve(), args.mode, args.action, args.repeats)

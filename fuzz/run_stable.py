#!/usr/bin/env python3
"""Run the stable stress test and capture source/binary identities at execution."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sources():
    paths = [ROOT / p for p in ("Cargo.toml", "Cargo.lock", "fuzz/Cargo.toml", "fuzz/Cargo.lock")]
    for folder, pattern in (("src", "*.rs"), ("data", "*.json"), ("fuzz", "*.rs")):
        paths += sorted((ROOT / folder).rglob(pattern))
    values = {p.relative_to(ROOT).as_posix(): sha(p) for p in paths}
    return dict(files_sha256=values, source_sha256=hashlib.sha256(json.dumps(values, sort_keys=True).encode()).hexdigest())


if __name__ == "__main__":
    before = sources()
    compiler = subprocess.check_output(["rustc", "-vV"], text=True).strip()
    command = ["cargo", "build", "--release", "--locked", "--manifest-path", "fuzz/Cargo.toml", "--bin", "hostile"]
    subprocess.run(command, cwd=ROOT, check=True)
    binary = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "release" / "hostile"
    identity = sha(binary)
    if sources() != before:
        raise RuntimeError("fuzz sources changed during build")
    result = subprocess.run([str(binary), "100000"], check=True, capture_output=True, text=True)
    if sources() != before or sha(binary) != identity:
        raise RuntimeError("fuzz sources or binary changed during execution")
    record = dict(**before, compiler=compiler, binary_sha256=identity, command=command,
                  seed="0x435656445046555a", cases_per_target=100000, exit_code=result.returncode,
                  method="stable hostile-input fallback; user toolchain directory is read-only", output=result.stdout)
    (ROOT / "fuzz" / "MEASURED_RESULTS.json").write_text(json.dumps(record, indent=2) + "\n")
    print(result.stdout, end="")

#!/usr/bin/env python3
"""Regenerates the images in docs/img/ and the browser demo's sample pair.

Run from the repository root:

    python3 docs/img/generate.py

Needs cargo, NumPy and Pillow (with WebP support). Scenes, scores and
distortion maps come from `examples/showcase.rs`; this script lays them out,
colours the maps and draws the benchmark chart from
`parity/MEASURED_PERFORMANCE.md`.
"""

import re
import subprocess
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
IMG = ROOT / "docs" / "img"
WEB = ROOT / "examples" / "web"

BACKGROUND = (246, 248, 250)
TEXT = (31, 35, 40)
FONT = ImageFont.load_default(size=15)
SMALL = ImageFont.load_default(size=13)
# Map values 1 - per-pixel JOD / 10 are shown from 0 (no difference) to
# MAP_MAX (per-pixel JOD 5) and clipped above.
MAP_MAX = 0.5
# Nine anchors approximating the Magma colour map, interpolated linearly.
MAGMA = np.array([
    [0, 0, 4], [28, 16, 68], [79, 18, 123], [129, 37, 129], [181, 54, 122],
    [229, 80, 100], [251, 135, 97], [254, 194, 135], [252, 253, 191],
], dtype=np.float32)


def colour(values):
    t = np.clip(values / MAP_MAX, 0.0, 1.0) * (len(MAGMA) - 1)
    i = np.minimum(t.astype(int), len(MAGMA) - 2)
    f = (t - i)[..., None]
    return (MAGMA[i] * (1 - f) + MAGMA[i + 1] * f).round().astype(np.uint8)


def legend(width, height=10):
    strip = colour(np.linspace(0, MAP_MAX, width, dtype=np.float32))
    return Image.fromarray(np.repeat(strip[None], height, axis=0))


class Showcase:
    def __init__(self, directory):
        self.dir = Path(directory)
        self.scores = {}
        for line in (self.dir / "scores.tsv").read_text().splitlines():
            name, jod, w, h, frames = line.split("\t")
            self.scores[name] = (float(jod), int(w), int(h), int(frames))

    def image(self, name):
        return Image.open(self.dir / f"{name}.ppm").convert("RGB")

    def maps(self, name):
        _, w, h, frames = self.scores[name]
        data = np.fromfile(self.dir / f"{name}.map.f32", dtype="<f4")
        return [Image.fromarray(colour(m)) for m in data.reshape(frames, h, w)]

    def jod(self, name):
        return self.scores[name][0]


def canvas(width, height):
    image = Image.new("RGB", (width, height), BACKGROUND)
    return image, ImageDraw.Draw(image)


def hero(s):
    ref, test, heat = s.image("hero-reference"), s.image("hero-test"), s.maps("hero")[0]
    w, h = ref.size
    pad, label = 16, 26
    image, draw = canvas(3 * w + 4 * pad, h + 2 * pad + label + 18)
    captions = ["Reference", f"Test: {s.jod('hero'):.2f} JOD", "Distortion map"]
    for i, (panel, caption) in enumerate(zip([ref, test, heat], captions)):
        x = pad + i * (w + pad)
        image.paste(panel, (x, pad))
        draw.text((x, pad + h + 6), caption, fill=TEXT, font=FONT)
    x = pad + 2 * (w + pad)
    bar_y = pad + h + label
    image.paste(legend(w - 120), (x + 60, bar_y + 3))
    draw.text((x, bar_y), "JOD 10", fill=TEXT, font=SMALL)
    draw.text((x + w - 44, bar_y), "JOD 5", fill=TEXT, font=SMALL)
    return image


def levels(s):
    ref = s.image("levels-reference")
    w, h = ref.size
    rows = [
        ("Noise", "sigma", ["0.01", "0.03", "0.08"]),
        ("Blur", "sigma", ["0.6 px", "1.2 px", "2.5 px"]),
        ("Blocking", "step", ["0.03", "0.1", "flat"]),
        ("Colour shift", "±", ["0.02", "0.05", "0.12"]),
    ]
    pad, left, label = 12, 104, 40
    image, draw = canvas(left + 4 * (w + pad) + pad, pad + len(rows) * (h + label))
    for r, (kind, symbol, strengths) in enumerate(rows):
        y = pad + r * (h + label)
        draw.text((pad, y + h // 2 - 8), kind, fill=TEXT, font=FONT)
        image.paste(ref, (left, y))
        draw.text((left, y + h + 4), "Reference", fill=TEXT, font=SMALL)
        draw.text((left, y + h + 19), "10.00 JOD", fill=TEXT, font=SMALL)
        key = {"Colour shift": "colour"}.get(kind, kind.lower())
        for i, strength in enumerate(strengths):
            name = f"levels-{key}-{i}"
            x = left + (i + 1) * (w + pad)
            image.paste(s.image(name), (x, y))
            draw.text((x, y + h + 4), f"{symbol} {strength}".replace("step flat", "flat blocks"),
                      fill=TEXT, font=SMALL)
            draw.text((x, y + h + 19), f"{s.jod(name):.2f} JOD", fill=TEXT, font=SMALL)
    return image


def video(s):
    _, w, h, frames = s.scores["video"]
    rows = [
        ("Reference", [s.image(f"video-reference-{f}") for f in range(frames)]),
        ("Test", [s.image(f"video-test-{f}") for f in range(frames)]),
        ("Map", s.maps("video")),
    ]
    pad, left, top = 8, 92, 32
    image, draw = canvas(left + frames * (w + pad) + pad, top + len(rows) * (h + pad) + pad)
    draw.text((left, 8), f"6 frames at 30 fps; the test flickers by ±3%: {s.jod('video'):.2f} JOD",
              fill=TEXT, font=FONT)
    for r, (name, tiles) in enumerate(rows):
        y = top + r * (h + pad)
        draw.text((pad, y + h // 2 - 8), name, fill=TEXT, font=FONT)
        for f, tile in enumerate(tiles):
            image.paste(tile, (left + f * (w + pad), y))
    return image


def save(image, name):
    image.save(IMG / f"{name}.webp", "WEBP", quality=90, method=6)


def performance():
    """Returns [(workload, threads, rust_ms, pytorch_ms)] from the record."""
    rows, threads = [], None
    for line in (ROOT / "parity" / "MEASURED_PERFORMANCE.md").read_text().splitlines():
        if line.startswith("Single thread"):
            threads = "1 thread"
        elif line.startswith("Multi-threaded"):
            threads = "16 threads"
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) >= 3 and re.fullmatch(r"[\d.]+", cells[1]):
            rows.append((cells[0].split(",")[0], threads, float(cells[1]), float(cells[2])))
    return rows


def chart():
    rows = performance()
    series = [("PyTorch CPU", "bar-py"), ("colorvideovdp", "bar-rs")]
    left, right, bar, gap, group_gap, plot_w, top = 150, 110, 14, 3, 18, 420, 70
    group_h = 2 * bar + gap
    height = top + len(rows) * (group_h + group_gap) + 40
    width = left + plot_w + right
    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}" font-family="-apple-system,BlinkMacSystemFont,'
        f'Segoe UI,Helvetica,Arial,sans-serif" font-size="12">',
        "<style>"
        "text{fill:#1f2328}.muted{fill:#59636e}.axis{stroke:#d1d9e0}"
        ".bar-py{fill:#8c959f}.bar-rs{fill:#e8743b}"
        "@media (prefers-color-scheme:dark){text{fill:#e6edf3}.muted{fill:#9198a1}"
        ".axis{stroke:#3d444d}.bar-py{fill:#768390}.bar-rs{fill:#f0883e}}"
        "</style>",
        '<text x="0" y="16" font-size="14" font-weight="600">'
        "Time per 1920×1080 prediction, relative to PyTorch on CPU (shorter is faster)</text>",
        '<text class="muted" x="0" y="34">Median of 3 runs, AMD Ryzen 9 7945HX, no distortion '
        "map. Video: 8 frames at 30 fps.</text>",
    ]
    x = left
    for name, cls in series:
        out.append(f'<rect class="{cls}" x="{x}" y="46" width="10" height="10"/>')
        out.append(f'<text x="{x + 14}" y="55">{name}</text>')
        x += 14 + 7 * len(name) + 24
    for g, (workload, threads, rust, torch) in enumerate(rows):
        y0 = top + g * (group_h + group_gap)
        out.append(f'<text x="{left - 8}" y="{y0 + group_h / 2 + 4}" text-anchor="end">'
                   f"{workload}, {threads}</text>")
        for i, ((_, cls), ms) in enumerate(zip(series, [torch, rust])):
            w = plot_w * ms / torch
            y = y0 + i * (bar + gap)
            out.append(f'<rect class="{cls}" x="{left}" y="{y}" width="{w:.1f}" height="{bar}"/>')
            out.append(f'<text x="{left + w + 6:.1f}" y="{y + bar - 3}">{ms / 1000:.2f} s</text>')
    axis_y = top + len(rows) * (group_h + group_gap) - group_gap + 6
    out.append(f'<line class="axis" x1="{left}" y1="{top - 4}" x2="{left}" y2="{axis_y}"/>')
    out.append(f'<text class="muted" x="0" y="{height - 8}">Measured before the hardening '
               "changes; not re-measured since. Data: parity/MEASURED_PERFORMANCE.md</text>")
    out.append("</svg>")
    (IMG / "benchmarks.svg").write_text("\n".join(out) + "\n")


def main():
    with tempfile.TemporaryDirectory() as tmp:
        subprocess.run(
            ["cargo", "run", "--release", "--example", "showcase", "--", tmp],
            cwd=ROOT, check=True,
        )
        s = Showcase(tmp)
        save(hero(s), "hero")
        save(levels(s), "jod-levels")
        save(video(s), "video-strip")
        s.image("hero-reference").save(WEB / "sample-reference.png", optimize=True)
        s.image("hero-test").save(WEB / "sample-test.png", optimize=True)
    chart()


if __name__ == "__main__":
    main()

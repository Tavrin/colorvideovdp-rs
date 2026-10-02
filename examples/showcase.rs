//! Computes the README figures' scenes, scores and distortion maps.
//!
//! Usage: `cargo run --release --example showcase -- <output-dir>`
//!
//! Writes 8-bit binary PPM images, raw little-endian f32 distortion maps and a
//! `scores.tsv` table (name, JOD, width, height, frames) to `<output-dir>`.
//! `docs/img/generate.py` turns them into the figures in `docs/img/`. All
//! scenes are procedural; the noise is seeded, so the output is deterministic.

use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use colorvideovdp::{Color, Cvvdp, DisplayModel, Image, Options, Prediction};

/// Display used for every figure: a 24-inch 1920 × 1080 monitor at 0.6 m.
const DISPLAY: &str = "standard_fhd";

type Res<T> = std::result::Result<T, Box<dyn Error>>;

/// A procedural outdoor scene in sRGB, with the moving parts offset by `t` pixels.
fn scene(width: usize, height: usize, t: f32) -> Res<Image> {
    let mut data = Vec::with_capacity(width * height * 3);
    let (w, h) = (width as f32, height as f32);
    let horizon = 0.58 * h;
    for y in 0..height {
        for x in 0..width {
            let (xf, yf) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut rgb = if yf < horizon {
                // Sky: blue at the top, pale near the horizon.
                let s = yf / horizon;
                [0.25 + 0.5 * s, 0.45 + 0.4 * s, 0.85 + 0.1 * s]
            } else {
                // Ground: green with a fine grass texture.
                let g = 0.5 + 0.5 * (xf * 0.9).sin() * (yf * 1.3).cos();
                [0.22 + 0.06 * g, 0.42 + 0.1 * g, 0.16]
            };
            // Sun, moving to the right with t.
            let (sx, sy, sr) = (0.2 * w + t, 0.22 * h, 0.08 * h);
            if (xf - sx).hypot(yf - sy) < sr {
                rgb = [1.0, 0.86, 0.4];
            }
            // A building with a window grid, standing on the horizon.
            let (bx0, bx1, by0) = (0.5 * w, 0.72 * w, 0.25 * h);
            if xf >= bx0 && xf < bx1 && yf >= by0 && yf < horizon {
                let window =
                    ((xf - bx0) as usize / 6) % 2 == 1 && ((yf - by0) as usize / 8) % 2 == 1;
                rgb = if window {
                    [0.95, 0.9, 0.62]
                } else {
                    [0.55, 0.36, 0.3]
                };
            }
            // A sign with a fine sinusoidal grating, and saturated colour bars.
            let (gx0, gx1, gy0, gy1) = (0.78 * w, 0.95 * w, 0.32 * h, 0.52 * h);
            if xf >= gx0 && xf < gx1 && yf >= gy0 && yf < gy1 {
                let v = 0.5 + 0.4 * (xf * 1.6).sin();
                rgb = [v, v, v];
            }
            let (cy0, cy1) = (0.74 * h, 0.86 * h);
            if yf >= cy0 && yf < cy1 && xf >= 0.08 * w && xf < 0.44 * w {
                let bar = ((xf - 0.08 * w) / (0.09 * w)) as usize;
                rgb = [
                    [0.85, 0.1, 0.1],
                    [0.1, 0.7, 0.15],
                    [0.12, 0.2, 0.85],
                    [0.95, 0.85, 0.1],
                ][bar.min(3)];
            }
            data.extend(rgb.map(quantize));
        }
    }
    Ok(Image::new(width, height, 3, data)?)
}

/// Rounds to 8-bit sRGB, so the scores match the images as saved.
fn quantize(c: f32) -> f32 {
    (c.clamp(0.0, 1.0) * 255.0).round() / 255.0
}

/// A small xorshift generator, so the figures do not depend on a crate version.
struct Rng(u64);

impl Rng {
    fn uniform(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 40) as f32 + 0.5) / (1u64 << 24) as f32
    }

    fn normal(&mut self) -> f32 {
        let (u, v) = (self.uniform(), self.uniform());
        (-2.0 * u.ln()).sqrt() * (std::f32::consts::TAU * v).cos()
    }
}

fn map_pixels(image: &Image, f: impl Fn(usize, usize, [f32; 3]) -> [f32; 3]) -> Res<Image> {
    let width = image.width();
    let data = image
        .data()
        .chunks_exact(3)
        .enumerate()
        .flat_map(|(i, p)| f(i % width, i / width, [p[0], p[1], p[2]]).map(quantize))
        .collect();
    Ok(Image::new(width, image.height(), 3, data)?)
}

/// Additive Gaussian noise with standard deviation `sigma`, on rows `rows`.
fn noise(image: &Image, sigma: f32, rows: std::ops::Range<usize>, seed: u64) -> Res<Image> {
    let rng = std::cell::RefCell::new(Rng(seed));
    map_pixels(image, |_, y, p| {
        if rows.contains(&y) {
            let mut rng = rng.borrow_mut();
            p.map(|c| c + sigma * rng.normal())
        } else {
            p
        }
    })
}

/// Separable Gaussian blur with standard deviation `sigma` pixels, on rows `rows`.
fn blur(image: &Image, sigma: f32, rows: std::ops::Range<usize>) -> Res<Image> {
    let (w, h) = (image.width(), image.height());
    let radius = (3.0 * sigma).ceil() as isize;
    let kernel: Vec<f32> = (-radius..=radius)
        .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f32 = kernel.iter().sum();
    let src = image.data();
    let at = |x: isize, y: isize, c: usize| {
        let x = x.clamp(0, w as isize - 1) as usize;
        let y = y.clamp(0, h as isize - 1) as usize;
        (x, y, c)
    };
    let mut horizontal = vec![0.0; src.len()];
    for y in 0..h as isize {
        for x in 0..w as isize {
            for c in 0..3 {
                let mut sum = 0.0;
                for (k, weight) in kernel.iter().enumerate() {
                    let (sx, sy, c) = at(x + k as isize - radius, y, c);
                    sum += weight * src[(sy * w + sx) * 3 + c];
                }
                horizontal[(y as usize * w + x as usize) * 3 + c] = sum / total;
            }
        }
    }
    map_pixels(image, |x, y, p| {
        if !rows.contains(&y) {
            return p;
        }
        let mut out = [0.0; 3];
        for (c, value) in out.iter_mut().enumerate() {
            let mut sum = 0.0;
            for (k, weight) in kernel.iter().enumerate() {
                let (sx, sy, c) = at(x as isize, y as isize + k as isize - radius, c);
                sum += weight * horizontal[(sy * w + sx) * 3 + c];
            }
            *value = sum / total;
        }
        out
    })
}

/// Compression-like blocking: each 8 × 8 block keeps its mean, and deviations
/// from the mean are quantized to multiples of `step`.
fn blocking(image: &Image, step: f32) -> Res<Image> {
    const BLOCK: usize = 8;
    let (w, h) = (image.width(), image.height());
    let src = image.data();
    let mut means = vec![[0.0f32; 3]; w.div_ceil(BLOCK) * h.div_ceil(BLOCK)];
    let mut counts = vec![0.0f32; means.len()];
    let block = |x: usize, y: usize| (y / BLOCK) * w.div_ceil(BLOCK) + x / BLOCK;
    for y in 0..h {
        for x in 0..w {
            let b = block(x, y);
            counts[b] += 1.0;
            for c in 0..3 {
                means[b][c] += src[(y * w + x) * 3 + c];
            }
        }
    }
    for (mean, count) in means.iter_mut().zip(&counts) {
        mean.iter_mut().for_each(|m| *m /= count);
    }
    map_pixels(image, |x, y, p| {
        let mean = means[block(x, y)];
        std::array::from_fn(|c| mean[c] + ((p[c] - mean[c]) / step).round() * step)
    })
}

/// A warm colour cast: red up and blue down by `amount`.
fn colour_shift(image: &Image, amount: f32) -> Res<Image> {
    map_pixels(image, |_, _, [r, g, b]| [r + amount, g, b - amount])
}

struct Output {
    dir: PathBuf,
    scores: String,
}

impl Output {
    fn image(&self, name: &str, image: &Image) -> Res<()> {
        let mut bytes = format!("P6\n{} {}\n255\n", image.width(), image.height()).into_bytes();
        bytes.extend(
            image
                .data()
                .iter()
                .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8),
        );
        fs::write(self.dir.join(format!("{name}.ppm")), bytes)?;
        Ok(())
    }

    fn prediction(&mut self, name: &str, prediction: &Prediction) -> Res<()> {
        let map = prediction
            .distortion_map
            .as_ref()
            .ok_or("distortion map missing")?;
        let bytes: Vec<u8> = map.data.iter().flat_map(|v| v.to_le_bytes()).collect();
        fs::write(self.dir.join(format!("{name}.map.f32")), bytes)?;
        writeln!(
            self.scores,
            "{name}\t{:.4}\t{}\t{}\t{}",
            prediction.jod, map.width, map.height, map.frames
        )?;
        println!("{name}: {:.4} JOD", prediction.jod);
        Ok(())
    }
}

fn main() -> Res<()> {
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: showcase <output-dir>")?;
    fs::create_dir_all(&dir)?;
    let mut out = Output {
        dir: Path::new(&dir).to_path_buf(),
        scores: String::new(),
    };
    let metric = Cvvdp::new(
        DisplayModel::from_name(DISPLAY)?,
        Options::default().with_distortion_map(true),
    )?;

    // Hero: noise in the sky, blur on the ground.
    let (w, h) = (384, 256);
    let reference = scene(w, h, 0.0)?;
    let horizon = (0.58 * h as f32) as usize;
    let test = blur(&noise(&reference, 0.04, 0..horizon, 1)?, 1.5, horizon..h)?;
    out.image("hero-reference", &reference)?;
    out.image("hero-test", &test)?;
    let prediction = metric.predict_image(&test, &reference, Color::Srgb)?;
    out.prediction("hero", &prediction)?;

    // The same scene at three strengths of four distortions.
    let (w, h) = (192, 128);
    let reference = scene(w, h, 0.0)?;
    out.image("levels-reference", &reference)?;
    let levels: [(&str, [f32; 3]); 4] = [
        ("noise", [0.01, 0.03, 0.08]),
        ("blur", [0.6, 1.2, 2.5]),
        ("blocking", [0.03, 0.1, 1.0]),
        ("colour", [0.02, 0.05, 0.12]),
    ];
    for (kind, strengths) in levels {
        for (i, s) in strengths.into_iter().enumerate() {
            let test = match kind {
                "noise" => noise(&reference, s, 0..h, 7 + i as u64)?,
                "blur" => blur(&reference, s, 0..h)?,
                "blocking" => blocking(&reference, s)?,
                _ => colour_shift(&reference, s)?,
            };
            let name = format!("levels-{kind}-{i}");
            out.image(&name, &test)?;
            let prediction = metric.predict_image(&test, &reference, Color::Srgb)?;
            out.prediction(&name, &prediction)?;
        }
    }

    // Video: the sun moves 4 pixels per frame at 30 fps; the test flickers
    // by ±3% in brightness on alternate frames.
    let (w, h, frames) = (160, 108, 6);
    let reference: Vec<Image> = (0..frames)
        .map(|f| scene(w, h, 4.0 * f as f32))
        .collect::<Res<_>>()?;
    let test: Vec<Image> = reference
        .iter()
        .enumerate()
        .map(|(f, frame)| {
            let gain = if f % 2 == 0 { 1.03 } else { 0.97 };
            map_pixels(frame, |_, _, p| p.map(|c| c * gain))
        })
        .collect::<Res<_>>()?;
    for (f, (r, t)) in reference.iter().zip(&test).enumerate() {
        out.image(&format!("video-reference-{f}"), r)?;
        out.image(&format!("video-test-{f}"), t)?;
    }
    let prediction = metric.predict_video(&test, &reference, 30.0, Color::Srgb)?;
    out.prediction("video", &prediction)?;

    fs::write(out.dir.join("scores.tsv"), &out.scores)?;
    Ok(())
}

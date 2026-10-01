use crate::allocation::collect;
use crate::plane::Plane;
use crate::Result;

const K: [f32; 5] = [0.05, 0.25, 0.4, 0.25, 0.05];

// lpyr_dec.py: lpyr_dec.__init__; the final spatial band is the baseband.
pub(crate) fn frequencies(width: usize, height: usize, ppd: f64) -> Result<Vec<f32>> {
    let max_levels = (width.min(height).ilog2() as usize).saturating_sub(1);
    let first_invalid = (0..15)
        .find(|&i| {
            let frequency = if i == 0 {
                1.0
            } else {
                0.3228 * 2.0_f64.powi(-(i - 1))
            };
            frequency * ppd / 2.0 <= 0.2
        })
        .unwrap_or(max_levels as i32) as usize;
    let levels = (first_invalid + 1).min(max_levels);
    let mut freqs = collect((0..levels + 1).map(|i| {
        Ok((if i == 0 {
            1.0
        } else {
            0.3228 * 2.0_f64.powi(-(i as i32 - 1))
        } * ppd
            / 2.0) as f32)
    }))?;
    freqs[levels] = 0.1;
    Ok(freqs)
}

// lpyr_dec.py: gausspyr_reduce, including its height-based horizontal end correction.
pub(crate) fn reduce(p: &Plane, parallel: bool) -> Result<Plane> {
    let vert = Plane::generate(p.w, p.h.div_ceil(2), parallel, |x, y| {
        let center = 2 * y;
        let mut sum = 0.0;
        for (k, &weight) in K.iter().enumerate() {
            let pos = center as isize + k as isize - 2;
            if pos >= 0 && pos < p.h as isize {
                sum += p.data[pos as usize * p.w + x] * weight;
            }
        }
        if y == 0 {
            sum += p.data[x] * K[1] + p.data[p.w + x] * K[0];
        }
        if y == p.h.div_ceil(2) - 1 {
            if p.h % 2 == 1 {
                sum += p.data[(p.h - 1) * p.w + x] * K[3] + p.data[(p.h - 2) * p.w + x] * K[4];
            } else {
                sum += p.data[(p.h - 1) * p.w + x] * K[4];
            }
        }
        sum
    })?;
    Plane::generate(p.w.div_ceil(2), vert.h, parallel, |x, y| {
        let center = 2 * x;
        let mut sum = 0.0;
        for (k, &weight) in K.iter().enumerate() {
            let pos = center as isize + k as isize - 2;
            if pos >= 0 && pos < p.w as isize {
                sum += vert.data[y * p.w + pos as usize] * weight;
            }
        }
        if x == 0 {
            sum += vert.data[y * p.w] * K[1] + vert.data[y * p.w + 1] * K[0];
        }
        if x == p.w.div_ceil(2) - 1 {
            if p.h % 2 == 1 {
                sum += vert.data[y * p.w + p.w - 1] * K[3] + vert.data[y * p.w + p.w - 2] * K[4];
            } else {
                sum += vert.data[y * p.w + p.w - 1] * K[4];
            }
        }
        sum
    })
}

fn interleaved_index(index: usize, expanded: usize, original: usize) -> Option<usize> {
    if index == 0 {
        Some(0)
    } else if index == expanded + 2 + expanded % 2 {
        Some(original - 1)
    } else if index >= 2 && index < expanded + 2 && index % 2 == 0 {
        Some((index - 2) / 2)
    } else {
        None
    }
}

// lpyr_dec.py: gausspyr_expand and interleave_zeros_and_pad.
pub(crate) fn expand(p: &Plane, width: usize, height: usize, parallel: bool) -> Result<Plane> {
    let vert = Plane::generate(p.w, height, parallel, |x, y| {
        let mut sum = 0.0;
        for (k, &weight) in K.iter().enumerate() {
            if let Some(pos) = interleaved_index(y + k, height, p.h) {
                sum += p.data[pos * p.w + x] * (2.0 * weight);
            }
        }
        sum
    })?;
    Plane::generate(width, height, parallel, |x, y| {
        let mut sum = 0.0;
        for (k, &weight) in K.iter().enumerate() {
            if let Some(pos) = interleaved_index(x + k, width, p.w) {
                sum += vert.data[y * p.w + pos] * (2.0 * weight);
            }
        }
        sum
    })
}

fn reflect(index: isize, size: usize) -> usize {
    if index < 0 {
        (-index) as usize
    } else if index >= size as isize {
        (2 * size as isize - index - 2) as usize
    } else {
        index as usize
    }
}

// cvvdp_metric.py: phase_uncertainty; torchvision GaussianBlur uses reflection padding.
pub(crate) fn blur(p: &Plane, kernel: &[f32], parallel: bool) -> Result<Plane> {
    let radius = kernel.len() / 2;
    let horizontal = Plane::generate(p.w, p.h, parallel, |x, y| {
        kernel
            .iter()
            .enumerate()
            .map(|(k, &v)| {
                p.data[y * p.w + reflect(x as isize + k as isize - radius as isize, p.w)] * v
            })
            .sum()
    })?;
    Plane::generate(p.w, p.h, parallel, |x, y| {
        kernel
            .iter()
            .enumerate()
            .map(|(k, &v)| {
                horizontal.data[reflect(y as isize + k as isize - radius as isize, p.h) * p.w + x]
                    * v
            })
            .sum()
    })
}

pub(crate) fn gaussian_kernel(sigma: f32) -> Result<Vec<f32>> {
    let radius = (sigma * 2.0) as usize;
    let mut kernel = collect(
        (0..2 * radius + 1)
            .map(|i| Ok((-0.5 * ((i as f32 - radius as f32) / sigma).powi(2)).exp())),
    )?;
    let sum: f32 = kernel.iter().sum();
    for x in &mut kernel {
        *x /= sum;
    }
    Ok(kernel)
}

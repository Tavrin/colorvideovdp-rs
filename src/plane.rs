#[cfg(feature = "parallel")]
use rayon::prelude::*;

use crate::allocation::filled;
use crate::parameters::norm;
#[cfg(feature = "parallel")]
use crate::parameters::safe_pow;
use crate::Result;

pub(crate) struct Plane {
    pub w: usize,
    pub h: usize,
    pub data: Vec<f32>,
}

impl Plane {
    pub fn zeros(w: usize, h: usize) -> Result<Self> {
        Ok(Self {
            w,
            h,
            data: filled(w.checked_mul(h).ok_or(crate::Error::SizeOverflow)?, 0.0)?,
        })
    }

    pub fn map_in_place(&mut self, parallel: bool, f: impl Fn(usize, usize, f32) -> f32 + Sync) {
        rows_mut(&mut self.data, self.w, parallel, |y, row| {
            for (x, value) in row.iter_mut().enumerate() {
                *value = f(x, y, *value);
            }
        });
    }

    // Only the independent powers run in parallel. Keep the f64 accumulation
    // in its original pixel order so that pooling remains bit-identical.
    // The caller must have finished using the unpooled differences.
    pub fn norm_in_place(&mut self, p: f32, parallel: bool) -> f32 {
        #[cfg(feature = "parallel")]
        if parallel && self.data.len() >= 4096 {
            rows_mut(&mut self.data, self.w, true, |_, row| {
                for value in row {
                    *value = safe_pow(*value, p);
                }
            });
            let sum: f64 = self.data.iter().map(|&x| x as f64).sum();
            return safe_pow((sum / self.data.len() as f64) as f32, 1.0 / p);
        }
        #[cfg(not(feature = "parallel"))]
        let _ = parallel;
        norm(self.data.iter().copied(), p, self.data.len())
    }

    pub fn generate(
        w: usize,
        h: usize,
        parallel: bool,
        f: impl Fn(usize, usize) -> f32 + Sync,
    ) -> Result<Self> {
        let mut plane = Self::zeros(w, h)?;
        rows_mut(&mut plane.data, w, parallel, |y, row| {
            for (x, value) in row.iter_mut().enumerate() {
                *value = f(x, y);
            }
        });
        Ok(plane)
    }
}

pub(crate) fn rows_mut(
    data: &mut [f32],
    width: usize,
    parallel: bool,
    f: impl Fn(usize, &mut [f32]) + Sync,
) {
    #[cfg(feature = "parallel")]
    if parallel && data.len() >= 4096 {
        data.par_chunks_mut(width)
            .enumerate()
            .for_each(|(y, row)| f(y, row));
        return;
    }
    #[cfg(not(feature = "parallel"))]
    let _ = parallel;
    data.chunks_mut(width)
        .enumerate()
        .for_each(|(y, row)| f(y, row));
}

#[cfg(feature = "parallel")]
use rayon::prelude::*;

use crate::allocation::filled;
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

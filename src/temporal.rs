use crate::allocation::collect;
use crate::parameters::Parameters;
use crate::{Error, Result, TemporalPadding};

/// Longest temporal filter; reached at the 16384 fps limit.
pub(crate) const MAX_FILTER_TAPS: usize = 4097;

pub(crate) fn filter_length(fps: f32) -> usize {
    (0.250 * fps as f64 / 2.0).ceil() as usize * 2 + 1
}

// cvvdp_metric.py: get_temporal_filters, real inverse FFT followed by fftshift.
pub(crate) fn filters(fps: f32, p: &Parameters) -> Result<[Vec<f32>; 4]> {
    let n = filter_length(fps);
    if n > MAX_FILTER_TAPS {
        return Err(Error::InvalidFrameRate(fps));
    }
    let frequencies = n / 2 + 1;
    let channels = collect((0..4).map(|channel| {
        let response = collect((0..frequencies).map(|i| {
            let omega = i as f32 * (fps / 2.0) / (frequencies - 1) as f32;
            Ok(if channel < 3 {
                (-omega.powf(p.beta_tf[channel]) / p.sigma_tf[channel]).exp()
            } else {
                (-(omega.powf(p.beta_tf[3]) - 5.0_f32.powf(p.beta_tf[3])).powi(2) / p.sigma_tf[3])
                    .exp()
            })
        }))?;
        let mut nonzero = crate::allocation::reserved(frequencies)?;
        nonzero.extend(
            response
                .iter()
                .copied()
                .enumerate()
                .skip(1)
                .filter(|&(_, r)| r != 0.0),
        );
        collect((0..n).map(|i| {
            let t = (i + n - n / 2) % n;
            let mut value = response[0] as f64;
            for &(k, r) in &nonzero {
                value +=
                    2.0 * r as f64 * (std::f64::consts::TAU * k as f64 * t as f64 / n as f64).cos();
            }
            Ok((value / n as f64) as f32)
        }))
    }))?;
    channels.try_into().map_err(|_| Error::NumericalFailure)
}

// cvvdp_metric.py: _get_symmetric_frame_index (reflection excludes the end sample).
// Only called for sequences of at least two frames, so `period` is non-zero.
pub(crate) fn frame_index(index: isize, count: usize, padding: TemporalPadding) -> usize {
    if index >= 0 {
        return index as usize;
    }
    match padding {
        TemporalPadding::Replicate => 0,
        TemporalPadding::Symmetric => {
            let period = 2 * (count - 1);
            let position = index.unsigned_abs() % period;
            if position < count {
                position
            } else {
                period - position
            }
        }
    }
}

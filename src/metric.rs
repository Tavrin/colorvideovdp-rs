use crate::allocation::{bytes, collect, filled, reserved};

use crate::color::transform;
use crate::csf::Csf;
use crate::display::ColorSpace;
use crate::parameters::{norm, safe_pow, Parameters};
use crate::plane::Plane;
use crate::pyramid::{blur, expand, frequencies, gaussian_kernel, reduce};
use crate::temporal::{filters, frame_index};
use crate::{
    Color, DisplayModel, DistortionMap, Error, Execution, Image, Options, Prediction, Result,
};

/// A reusable, immutable ColorVideoVDP evaluator for one display.
///
/// Construction loads the embedded calibration (base model
/// [`MODEL_VERSION`](Self::MODEL_VERSION)); predictions take `&self`, so one
/// evaluator can serve many image pairs or sequences, including from several
/// threads.
pub struct Cvvdp {
    display: DisplayModel,
    options: Options,
    parameters: Parameters,
    csf: Csf,
    blur_kernel: Vec<f32>,
    xcm: [f32; 16],
}

impl std::fmt::Debug for Cvvdp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cvvdp")
            .field("display", &self.display)
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

// Highest frame rate whose temporal filters fit in `temporal::MAX_FILTER_TAPS`.
const MAX_FPS: f32 = 16384.0;

// Direct slots give O(taps + live frames) cache maintenance per output frame.
struct FrameCache {
    slots: Vec<Option<Vec<Plane>>>,
    needed: Vec<bool>,
    active: Vec<usize>,
}

impl FrameCache {
    fn new(frames: usize, taps: usize) -> Result<Self> {
        Ok(Self {
            slots: collect((0..frames).map(|_| Ok(None)))?,
            needed: filled(frames, false)?,
            active: reserved(frames.min(taps))?,
        })
    }

    fn update(
        &mut self,
        indices: &[usize],
        mut convert: impl FnMut(usize) -> Result<Vec<Plane>>,
    ) -> Result<()> {
        for &index in indices {
            self.needed[index] = true;
        }
        self.active.retain(|&index| {
            if self.needed[index] {
                true
            } else {
                self.slots[index] = None;
                false
            }
        });
        for &index in indices {
            if self.slots[index].is_none() {
                self.slots[index] = Some(convert(index)?);
                self.active.push(index);
            }
        }
        for &index in indices {
            self.needed[index] = false;
        }
        Ok(())
    }
}

// Conservative bound: converted cache + 96 full-size working planes (including
// all simultaneously live pyramid, masking and heatmap planes) + outputs. Every
// level shrinks geometrically; even mixed parity sums to less than 2 full planes.
fn memory_required(
    pixels: usize,
    frames: usize,
    bands: usize,
    channels: usize,
    taps: usize,
    maps: bool,
) -> Result<usize> {
    let cached = if frames > 1 { frames.min(taps) } else { 0 };
    let planes = cached
        .checked_mul(6)
        .and_then(|n| n.checked_add(96))
        .ok_or(Error::SizeOverflow)?;
    let working = bytes::<f32>(pixels.checked_mul(planes).ok_or(Error::SizeOverflow)?)?;
    let features = bytes::<f32>(
        channels
            .checked_mul(frames)
            .and_then(|n| n.checked_mul(bands))
            .ok_or(Error::SizeOverflow)?,
    )?;
    let map = if maps {
        bytes::<f32>(pixels.checked_mul(frames).ok_or(Error::SizeOverflow)?)?
    } else {
        0
    };
    let cache_records = bytes::<Option<Vec<Plane>>>(frames)?;
    // Covers slot flags, active indices, history indices/references, Plane
    // descriptors, filter buffers and CSF tables. Calibration itself is excluded.
    let metadata = frames
        .checked_mul(32)
        .and_then(|n| n.checked_add(cached * 256))
        .and_then(|n| n.checked_add(1_048_576))
        .ok_or(Error::SizeOverflow)?;
    [working, features, map, cache_records, metadata]
        .into_iter()
        .try_fold(0usize, |sum, n| {
            sum.checked_add(n).ok_or(Error::SizeOverflow)
        })
}

struct FrameResult {
    qualities: Vec<f32>,
    map: Option<Vec<f32>>,
}

impl Cvvdp {
    /// Version of the reference base-model calibration.
    pub const MODEL_VERSION: &'static str = "0.5.7";

    /// Construct an evaluator for a display.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidDisplay`] or [`Error::UnknownName`] if the display
    /// photometry or default colour space is invalid, and
    /// [`Error::ParallelUnavailable`] if [`Execution::Parallel`] is requested
    /// without the `parallel` feature.
    pub fn new(display: DisplayModel, options: Options) -> Result<Self> {
        display.validate()?;
        if options.execution == Execution::Parallel && !cfg!(feature = "parallel") {
            return Err(Error::ParallelUnavailable);
        }
        let parameters = Parameters::load()?;
        let blur_kernel = gaussian_kernel(parameters.pu_dilate)?;
        let xcm = parameters.xcm_weights.map(|x| 2.0_f32.powf(x));
        Ok(Self {
            display,
            options,
            parameters,
            csf: Csf::load()?,
            blur_kernel,
            xcm,
        })
    }

    /// Display used by this evaluator.
    pub fn display(&self) -> &DisplayModel {
        &self.display
    }

    /// Options used by this evaluator.
    pub fn options(&self) -> &Options {
        &self.options
    }

    /// Predict quality, and optionally a distortion map, for a pair of images.
    ///
    /// Single-channel inputs follow the reference's broadcast across its three
    /// sustained channels.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ShapeMismatch`] if the images differ in shape,
    /// [`Error::NonFiniteInput`] for NaN or infinite samples,
    /// [`Error::UnknownName`] or [`Error::ColorChannelMismatch`] for an
    /// unusable colour space, [`Error::InvalidDisplay`] for HLG input on a
    /// display above 1000 cd/m² without ambient light, and
    /// [`Error::NumericalFailure`] if the result is not finite. Returns
    /// [`Error::SizeOverflow`] for unsupported byte layouts,
    /// [`Error::MemoryLimitExceeded`] if the conservative storage estimate
    /// exceeds the configured budget, or [`Error::AllocationFailed`] if a
    /// prediction buffer cannot be reserved.
    pub fn predict_image(
        &self,
        test: &Image,
        reference: &Image,
        color: Color,
    ) -> Result<Prediction> {
        self.predict(
            std::slice::from_ref(test),
            std::slice::from_ref(reference),
            0.0,
            color,
        )
    }

    /// Predict quality, and optionally a distortion map, for two frame sequences.
    ///
    /// Frames are processed in order, keeping only the converted frames that the
    /// temporal filters still need. One-frame sequences take the image path, as
    /// in the reference. Temporal filters are limited to 4097 taps, which caps
    /// the frame rate at 16384 fps.
    ///
    /// # Errors
    ///
    /// As [`predict_image`](Self::predict_image), plus
    /// [`Error::EmptySequence`], [`Error::InvalidFrameRate`] unless
    /// `0 < fps <= 16384`, and [`Error::ShapeMismatch`] if the sequences differ
    /// in length or any frame differs in shape.
    pub fn predict_video(
        &self,
        test: &[Image],
        reference: &[Image],
        fps: f32,
        color: Color,
    ) -> Result<Prediction> {
        if !fps.is_finite() || fps <= 0.0 || fps > MAX_FPS {
            return Err(Error::InvalidFrameRate(fps));
        }
        self.predict(test, reference, fps, color)
    }

    fn parallel(&self) -> bool {
        self.options.execution == Execution::Parallel
    }

    fn convert(&self, test: &Image, reference: &Image, space: &ColorSpace) -> Result<Vec<Plane>> {
        let t = transform(test, &self.display.photometry, space, self.parallel())?;
        let r = transform(reference, &self.display.photometry, space, self.parallel())?;
        let mut output = reserved(6)?;
        for (t, r) in t.into_iter().zip(r) {
            output.extend([t, r]);
        }
        Ok(output)
    }

    fn predict(
        &self,
        test: &[Image],
        reference: &[Image],
        fps: f32,
        color: Color,
    ) -> Result<Prediction> {
        // Enter the current pool once: each spatial pass then forks locally,
        // rather than handing work back and forth from an external caller.
        #[cfg(feature = "parallel")]
        if self.parallel() {
            return rayon::scope(|_| self.predict_inner(test, reference, fps, color));
        }
        self.predict_inner(test, reference, fps, color)
    }

    fn predict_inner(
        &self,
        test: &[Image],
        reference: &[Image],
        fps: f32,
        color: Color,
    ) -> Result<Prediction> {
        let first = test.first().ok_or(Error::EmptySequence)?;
        if test.len() != reference.len()
            || test.iter().chain(reference).any(|x| !x.same_shape(first))
        {
            return Err(Error::ShapeMismatch);
        }
        if test
            .iter()
            .chain(reference)
            .any(|x| x.data().iter().any(|v| !v.is_finite()))
        {
            return Err(Error::NonFiniteInput);
        }
        let space = color.resolve(&self.display.color_space)?;
        let (w, h, frames) = (first.width(), first.height(), test.len());
        let pixels = w * h;
        let bands = frequencies(w, h, self.display.geometry.pixels_per_degree())?;
        let channels: usize = if frames == 1 { 3 } else { 4 };
        let feature_count = channels
            .checked_mul(frames)
            .and_then(|n| n.checked_mul(bands.len()))
            .ok_or(Error::SizeOverflow)?;
        let taps = if frames > 1 {
            crate::temporal::filter_length(fps)
        } else {
            0
        };
        let required = memory_required(
            pixels,
            frames,
            bands.len(),
            channels,
            taps,
            self.options.distortion_map,
        )?;
        if required > self.options.memory_limit_bytes {
            return Err(Error::MemoryLimitExceeded {
                required,
                limit: self.options.memory_limit_bytes,
            });
        }
        let mut quality = filled(feature_count, 0.0)?;
        let mut map = if self.options.distortion_map {
            Some(reserved(
                pixels.checked_mul(frames).ok_or(Error::SizeOverflow)?,
            )?)
        } else {
            None
        };
        let filter = if frames > 1 {
            Some(filters(fps, &self.parameters)?)
        } else {
            None
        };
        let csf_tables = collect(bands.iter().map(|&f| self.csf.at_frequency(f)))?;
        let mut cache = FrameCache::new(if frames > 1 { frames } else { 0 }, taps)?;
        for f in 0..frames {
            let current = if let Some(filters) = &filter {
                let length = filters[0].len();
                let indices = collect((0..length).map(|i| {
                    Ok(frame_index(
                        f as isize + i as isize - length as isize + 1,
                        frames,
                        self.options.temporal_padding,
                    ))
                }))?;
                cache.update(&indices, |index| {
                    self.convert(&test[index], &reference[index], &space)
                })?;
                let history = collect(
                    indices
                        .iter()
                        .map(|&index| cache.slots[index].as_ref().ok_or(Error::NumericalFailure)),
                )?;
                collect((0..8).map(|c| {
                    let channel = c / 2;
                    let source = if channel == 3 { c % 2 } else { c };
                    Plane::generate(w, h, self.parallel(), |x, y| {
                        let i = y * w + x;
                        let mut sum = 0.0;
                        for (j, entry) in history.iter().enumerate() {
                            sum += entry[source].data[i] * filters[channel][length - j - 1];
                        }
                        sum
                    })
                }))?
            } else {
                self.convert(first, &reference[0], &space)?
            };
            let result = self.process_frame(current, &csf_tables, frames == 1)?;
            for c in 0..channels {
                for b in 0..bands.len() {
                    quality[(c * frames + f) * bands.len() + b] =
                        result.qualities[c * bands.len() + b];
                }
            }
            if let (Some(map), Some(frame)) = (&mut map, result.map) {
                map.extend(frame);
            }
        }
        let jod = self.pool(&quality, channels, frames, bands.len());
        if !jod.is_finite()
            || quality.iter().any(|v| !v.is_finite())
            || map
                .as_ref()
                .is_some_and(|m| m.iter().any(|v| !v.is_finite()))
        {
            return Err(Error::NumericalFailure);
        }
        Ok(Prediction {
            jod,
            distortion_map: map.map(|data| DistortionMap {
                width: w,
                height: h,
                frames,
                data,
            }),
            channels,
            frames,
            band_frequencies: bands,
            quality_per_channel: quality,
        })
    }

    // cvvdp_metric.py: do_pooling_and_jods.
    fn pool(&self, quality: &[f32], channels: usize, frames: usize, bands: usize) -> f32 {
        let p = &self.parameters;
        let weights = p.channel_weights();
        let mut per_frame = (0..frames).map(|f| {
            norm(
                (0..channels).map(|c| {
                    norm(
                        (0..bands).map(|b| {
                            let weight = if b == bands - 1 {
                                p.baseband_weight[c]
                            } else {
                                1.0
                            };
                            quality[(c * frames + f) * bands + b] * weights[c] * weight
                        }),
                        p.beta_sch,
                        1,
                    )
                }),
                p.beta_tch,
                1,
            )
        });
        let q = if frames == 1 {
            per_frame.next().unwrap_or(0.0) * p.image_int
        } else {
            norm(per_frame, p.beta_t, frames)
        };
        p.jod(q)
    }

    // lpyr_dec.py: weber_contrast_pyr.decompose; cvvdp_metric.py: process_block_of_frames.
    fn process_frame(
        &self,
        mut gaussian: Vec<Plane>,
        tables: &[[Vec<f32>; 4]],
        is_image: bool,
    ) -> Result<FrameResult> {
        let p = &self.parameters;
        let channels = gaussian.len() / 2;
        let mut qualities = filled(channels * tables.len(), 0.0)?;
        let mut heatmap_bands = reserved(tables.len())?;
        for (band, table) in tables.iter().enumerate() {
            let baseband = band == tables.len() - 1;
            let w = gaussian[0].w;
            let h = gaussian[0].h;
            let next: Vec<Plane> = if baseband {
                Vec::new()
            } else {
                collect(gaussian.iter().map(|g| reduce(g, self.parallel())))?
            };
            let expanded: Vec<Plane> = if baseband {
                Vec::new()
            } else {
                collect(next.iter().map(|g| expand(g, w, h, self.parallel())))?
            };
            let backgrounds: Vec<Plane> = if baseband {
                collect(gaussian[..2].iter().map(|g| {
                    let sum: f64 = g.data.iter().map(|&x| x.max(0.01) as f64).sum();
                    Plane::generate(w, h, self.parallel(), |_, _| (sum / (w * h) as f64) as f32)
                }))?
            } else {
                collect(expanded[..2].iter().map(|g| {
                    Plane::generate(w, h, self.parallel(), |x, y| g.data[y * w + x].max(0.01))
                }))?
            };
            let gain = if band == 0 || baseband { 1.0 } else { 2.0 };
            let contrasts: Vec<Plane> = collect(gaussian.iter().enumerate().map(|(c, g)| {
                Plane::generate(w, h, self.parallel(), |x, y| {
                    let i = y * w + x;
                    let layer = if baseband {
                        g.data[i]
                    } else {
                        g.data[i] - expanded[c].data[i]
                    };
                    (layer / backgrounds[c % 2].data[i]).min(1000.0) * gain
                })
            }))?;
            let correction = 10.0_f32.powf(p.sensitivity_correction / 20.0);
            let sensitivity: Vec<Plane> = collect((0..channels).map(|c| {
                Plane::generate(w, h, self.parallel(), |x, y| {
                    self.csf
                        .sensitivity(&table[c], backgrounds[1].data[y * w + x].log10())
                        * correction
                })
            }))?;
            let mut differences = if baseband {
                collect((0..channels).map(|c| {
                    Plane::generate(w, h, self.parallel(), |x, y| {
                        let i = y * w + x;
                        (contrasts[2 * c].data[i] - contrasts[2 * c + 1].data[i]).abs()
                            * sensitivity[c].data[i]
                    })
                }))?
            } else {
                self.mask(contrasts, &sensitivity)?
            };
            if differences
                .iter()
                .any(|d: &Plane| d.data.iter().any(|v| !v.is_finite()))
            {
                return Err(Error::NumericalFailure);
            }
            if self.options.distortion_map {
                let weights = p.channel_weights();
                let integration = if is_image { p.image_int } else { 1.0 };
                heatmap_bands.push(Plane::generate(w, h, self.parallel(), |x, y| {
                    let i = y * w + x;
                    let d = norm(
                        (0..channels).map(|c| {
                            let weight = weights[c]
                                * integration
                                * if baseband { p.baseband_weight[c] } else { 1.0 };
                            differences[c].data[i] * weight
                        }),
                        p.beta_tch,
                        1,
                    );
                    d / gain
                })?);
            }
            // Maps consume the original differences before pooling reuses them.
            for c in 0..channels {
                qualities[c * tables.len() + band] =
                    differences[c].norm_in_place(p.beta, self.parallel());
            }
            gaussian = next;
        }
        let map = if self.options.distortion_map {
            let mut reconstructed = heatmap_bands.pop().ok_or(Error::NumericalFailure)?;
            for band in heatmap_bands.into_iter().rev() {
                let mut larger = expand(&reconstructed, band.w, band.h, self.parallel())?;
                for (a, b) in larger.data.iter_mut().zip(band.data) {
                    *a += b;
                }
                reconstructed = larger;
            }
            for q in &mut reconstructed.data {
                *q = 1.0 - p.jod(*q) / 10.0;
            }
            Some(reconstructed.data)
        } else {
            None
        };
        Ok(FrameResult { qualities, map })
    }

    // cvvdp_metric.py: apply_masking_model (mult-mutual), phase_uncertainty, mask_pool, clamp_diffs.
    fn mask(&self, contrasts: Vec<Plane>, sensitivity: &[Plane]) -> Result<Vec<Plane>> {
        let p = &self.parameters;
        let (w, h) = (contrasts[0].w, contrasts[0].h);
        let channels = sensitivity.len();
        let gains = [1.0, 1.45, 1.0, 1.0];
        // These contrasts are no longer needed after normalization. Reuse their
        // fallibly allocated storage instead of serially zeroing new planes.
        let mut normalized = contrasts;
        for (c, plane) in normalized.iter_mut().enumerate() {
            plane.map_in_place(self.parallel(), |x, y, value| {
                value * sensitivity[c / 2].data[y * w + x] * gains[c / 2]
            });
        }
        let mask_scale = 10.0_f32.powf(p.mask_c);
        let masking: Vec<Plane> = collect((0..channels).map(|c| {
            let mutual = Plane::generate(w, h, self.parallel(), |x, y| {
                let i = y * w + x;
                normalized[2 * c].data[i]
                    .abs()
                    .min(normalized[2 * c + 1].data[i].abs())
            })?;
            let mut mutual = if w > self.blur_kernel.len() / 2 && h > self.blur_kernel.len() / 2 {
                blur(&mutual, &self.blur_kernel, self.parallel())?
            } else {
                mutual
            };
            mutual.map_in_place(self.parallel(), |_, _, value| {
                safe_pow((value * mask_scale).abs(), p.mask_q[c])
            });
            Ok(mutual)
        }))?;
        let max_d = 10.0_f32.powf(p.d_max);
        let mut pairs = normalized.into_iter();
        collect((0..channels).map(|c| {
            let mut test = pairs.next().ok_or(Error::NumericalFailure)?;
            let reference = pairs.next().ok_or(Error::NumericalFailure)?;
            test.map_in_place(self.parallel(), |x, y, value| {
                let i = y * w + x;
                let mut m = 0.0;
                for (source, mask) in masking.iter().enumerate() {
                    m += mask.data[i] * self.xcm[source * 4 + c];
                }
                let diff = (value - reference.data[i]).abs();
                let d = safe_pow(diff, p.mask_p) / (1.0 + m);
                max_d * d / (max_d + d)
            });
            Ok(test)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_indexes_4097_distinct_frames_and_evicts() -> Result<()> {
        let mut cache = FrameCache::new(4100, 4097)?;
        let indices: Vec<_> = (0..4097).rev().collect();
        let mut conversions = 0;
        cache.update(&indices, |_| {
            conversions += 1;
            Ok(Vec::new())
        })?;
        assert_eq!(conversions, 4097);
        cache.update(&indices, |_| {
            conversions += 1;
            Ok(Vec::new())
        })?;
        assert_eq!(conversions, 4097);
        cache.update(&[4096, 4096, 4097], |_| {
            conversions += 1;
            Ok(Vec::new())
        })?;
        assert_eq!(conversions, 4098);
        assert_eq!(cache.active.len(), 2);
        assert!(cache.slots[..4096].iter().all(Option::is_none));
        Ok(())
    }

    #[test]
    fn prediction_layout_rejects_wasm_map_capacity_without_allocation() {
        // The complete conservative bound exceeds even a 64-bit byte layout at
        // this size. Both output and working storage use the same checked helper.
        assert_eq!(
            memory_required(usize::MAX / 4, 2, 2, 4, 3, true),
            Err(Error::SizeOverflow)
        );
    }
}

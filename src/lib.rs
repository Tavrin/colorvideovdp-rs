//! A pure-Rust port of ColorVideoVDP, a full-reference visual difference
//! predictor for images and video.
//!
//! ColorVideoVDP (the authors call it `cvvdp`) compares a test image or video
//! with a reference, as seen on a specified display, and predicts the visible
//! difference in Just-Objectionable-Difference (JOD) units. 10 JOD means no
//! visible difference, and lower values mean stronger distortion; very strong
//! distortions can give negative values. A difference of 1 JOD between two
//! conditions means that 75% of observers would choose the one with the higher
//! score. This crate implements the calibrated base model v0.5.7
//! ([`Cvvdp::MODEL_VERSION`]) of the official PyTorch implementation.
//!
//! ```
//! use colorvideovdp::{Color, Cvvdp, DisplayModel, Image, Options};
//!
//! # fn main() -> colorvideovdp::Result<()> {
//! let reference = Image::new(16, 16, 3, vec![0.5; 16 * 16 * 3])?;
//! let mut test = reference.clone();
//! test.data_mut()[0] = 0.6;
//!
//! let metric = Cvvdp::new(DisplayModel::from_name("standard_4k")?, Options::default())?;
//! let prediction = metric.predict_image(&test, &reference, Color::Srgb)?;
//! assert!(prediction.jod < 10.0);
//! # Ok(())
//! # }
//! ```
//!
//! # Algorithm
//!
//! 1. The display model maps encoded or linear input to absolute luminance
//!    (EOTF, peak luminance, black level, reflected ambient light), then to the
//!    DKL opponent space: achromatic, red–green, and yellow–violet.
//! 2. For video, temporal filters split each frame into sustained channels and
//!    an achromatic transient channel.
//! 3. Each channel is decomposed into a Laplacian pyramid and converted to
//!    contrast relative to the local background luminance.
//! 4. Contrast is weighted by castleCSF sensitivity for each band's spatial
//!    frequency and the local luminance.
//! 5. Test–reference differences are reduced by cross-channel masking, with a
//!    blur that models phase uncertainty, and soft-clamped.
//! 6. Differences are pooled with Lp norms over space, bands, channels, and
//!    frames, and the pooled value is mapped to JOD.
//!
//! # Inputs and displays
//!
//! [`Image`] holds one f32 frame. Encoded inputs (sRGB, PQ, HLG, gamma) are
//! clamped to [0, 1]; linear inputs are absolute luminance in cd/m². The
//! [`DisplayModel`] sets resolution, viewing geometry, and photometry; the 26
//! reference displays are embedded and listed by [`DisplayModel::names`].
//!
//! # Features
//!
//! - `parallel` (default): [`Execution::Parallel`] splits independent spatial
//!   rows across the Rayon thread pool. Results are identical to
//!   [`Execution::SingleThread`].
//!
//! # Reference
//!
//! Rafał K. Mantiuk, Param Hanji, Maliha Ashraf, Yuta Asano, and Alexandre
//! Chapiro. ColorVideoVDP: A visual difference predictor for image, video and
//! display distortions. In SIGGRAPH 2024 Technical Papers, Article 129.
//! <https://doi.org/10.1145/3658144>
//!
//! The official implementation is <https://github.com/gfxdisp/ColorVideoVDP>.

mod allocation;
mod color;
mod csf;
mod display;
mod error;
mod image;
mod metric;
mod parameters;
mod plane;
mod pyramid;
mod temporal;

pub use display::{Color, DisplayModel, Geometry, Photometry};
pub use error::{Error, Result};
pub use image::Image;
pub use metric::Cvvdp;

/// Padding before the first video frame during temporal filtering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TemporalPadding {
    /// Repeat the first frame (the reference default).
    #[default]
    Replicate,
    /// Reflect without repeating the endpoints; short clips reflect repeatedly.
    Symmetric,
}

/// CPU execution policy. Pooling order is deterministic in both modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Execution {
    /// Run all work on the calling thread.
    SingleThread,
    /// Split spatial rows across the current Rayon pool; requires the
    /// `parallel` feature.
    Parallel,
}

impl Default for Execution {
    /// [`Execution::Parallel`] with the `parallel` feature, otherwise
    /// [`Execution::SingleThread`].
    fn default() -> Self {
        if cfg!(feature = "parallel") {
            Self::Parallel
        } else {
            Self::SingleThread
        }
    }
}

/// Prediction options.
///
/// ```
/// use colorvideovdp::{Execution, Options, TemporalPadding};
///
/// let options = Options::default()
///     .with_distortion_map(true)
///     .with_temporal_padding(TemporalPadding::Symmetric)
///     .with_execution(Execution::SingleThread);
/// assert!(options.distortion_map);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Options {
    /// Return the raw per-pixel distortion map in addition to JOD.
    pub distortion_map: bool,
    /// Padding before the first frame of a sequence.
    pub temporal_padding: TemporalPadding,
    /// CPU execution policy.
    pub execution: Execution,
    /// Conservative upper bound on prediction buffer bytes, excluding caller-owned
    /// inputs and evaluator calibration. Defaults to 1 GiB.
    pub memory_limit_bytes: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            distortion_map: false,
            temporal_padding: TemporalPadding::default(),
            execution: Execution::default(),
            memory_limit_bytes: 1024 * 1024 * 1024,
        }
    }
}

impl Options {
    /// Set the conservative prediction memory limit, in bytes.
    #[must_use]
    pub fn with_memory_limit_bytes(mut self, bytes: usize) -> Self {
        self.memory_limit_bytes = bytes;
        self
    }

    /// Set [`Options::distortion_map`].
    #[must_use]
    pub fn with_distortion_map(mut self, distortion_map: bool) -> Self {
        self.distortion_map = distortion_map;
        self
    }

    /// Set [`Options::temporal_padding`].
    #[must_use]
    pub fn with_temporal_padding(mut self, temporal_padding: TemporalPadding) -> Self {
        self.temporal_padding = temporal_padding;
        self
    }

    /// Set [`Options::execution`].
    #[must_use]
    pub fn with_execution(mut self, execution: Execution) -> Self {
        self.execution = execution;
        self
    }
}

/// A raw distortion map in contiguous frame, row, column order.
///
/// Values are `1 - per_pixel_JOD / 10`, as in the reference `heatmap="raw"`
/// output. They are not clipped to [0, 1] and are kept in f32; the reference
/// quantizes its map to f16. Colour visualization is left to the caller.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DistortionMap {
    /// Width of each frame in pixels.
    pub width: usize,
    /// Height of each frame in pixels.
    pub height: usize,
    /// Number of map frames.
    pub frames: usize,
    /// One value per pixel per frame.
    pub data: Vec<f32>,
}

/// A quality score, an optional raw distortion map, and pooled features.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Prediction {
    /// Quality in JOD units: 10 for no visible difference, lower is worse.
    pub jod: f32,
    /// Raw per-pixel map, when requested with [`Options::distortion_map`].
    pub distortion_map: Option<DistortionMap>,
    /// Number of visual channels: 3 for images, 4 for video (the extra one is
    /// the achromatic transient channel).
    pub channels: usize,
    /// Number of input frames.
    pub frames: usize,
    /// Spatial band frequencies in cycles per degree; the last entry is the
    /// 0.1 cpd baseband.
    pub band_frequencies: Vec<f32>,
    /// Spatially pooled differences in channel, frame, band order; the
    /// reference's `Q_per_ch`.
    pub quality_per_channel: Vec<f32>,
}

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

use std::fmt;

/// An invalid input, display configuration, or numerical result.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// The image is smaller than 4 × 4, does not have 1 or 3 channels, or its
    /// buffer length is not `width * height * channels`.
    InvalidImage {
        /// Requested width in pixels.
        width: usize,
        /// Requested height in pixels.
        height: usize,
        /// Requested number of interleaved channels.
        channels: usize,
        /// Length of the supplied buffer.
        len: usize,
    },
    /// A three-channel image was given with a luminance-only colour space.
    ColorChannelMismatch,
    /// Test and reference shapes or sequence lengths differ.
    ShapeMismatch,
    /// A video sequence has no frames.
    EmptySequence,
    /// The frame rate is not in (0, 16384].
    InvalidFrameRate(f32),
    /// A buffer byte layout exceeds `isize::MAX` or a size calculation overflows.
    SizeOverflow,
    /// A fallible buffer reservation failed.
    AllocationFailed,
    /// The conservative prediction memory estimate exceeds the configured limit.
    MemoryLimitExceeded {
        /// Estimated maximum prediction buffer bytes.
        required: usize,
        /// Configured byte limit.
        limit: usize,
    },
    /// A pixel value is NaN or infinite.
    NonFiniteInput,
    /// A named display or colour space does not exist.
    UnknownName(String),
    /// A display configuration is invalid.
    InvalidDisplay(String),
    /// Embedded or supplied JSON could not be decoded.
    InvalidConfig(String),
    /// The prediction overflowed or otherwise became non-finite.
    NumericalFailure,
    /// [`Execution::Parallel`](crate::Execution::Parallel) was requested
    /// without the `parallel` feature.
    ParallelUnavailable,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidImage {
                width,
                height,
                channels,
                len,
            } => write!(
                f,
                "invalid image {width} × {height} × {channels} with {len} samples; \
                 expected at least 4 × 4, 1 or 3 channels, and an exact buffer length"
            ),
            Self::ColorChannelMismatch => {
                f.write_str("a luminance colour space requires single-channel images")
            }
            Self::ShapeMismatch => f.write_str("test and reference shapes differ"),
            Self::EmptySequence => f.write_str("the frame sequence is empty"),
            Self::InvalidFrameRate(fps) => {
                write!(f, "frame rate {fps} is outside (0, 16384]")
            }
            Self::SizeOverflow => f.write_str("buffer byte capacity overflow"),
            Self::AllocationFailed => f.write_str("prediction buffer allocation failed"),
            Self::MemoryLimitExceeded { required, limit } => write!(
                f,
                "prediction needs up to {required} buffer bytes; limit is {limit}"
            ),
            Self::NonFiniteInput => f.write_str("pixels must be finite"),
            Self::UnknownName(s) => write!(f, "unknown display or colour space: {s}"),
            Self::InvalidDisplay(s) => write!(f, "invalid display: {s}"),
            Self::InvalidConfig(s) => write!(f, "invalid configuration: {s}"),
            Self::NumericalFailure => f.write_str("non-finite prediction"),
            Self::ParallelUnavailable => f.write_str("the `parallel` feature is disabled"),
        }
    }
}

impl std::error::Error for Error {}

pub(crate) fn json_error(error: serde_json::Error) -> Error {
    Error::InvalidConfig(error.to_string())
}

/// Result type returned by this crate.
pub type Result<T> = std::result::Result<T, Error>;

use crate::{Error, Result};

/// An owned f32 image in row-major, interleaved RGB or single-channel layout.
///
/// Dimensions must be at least 4 × 4. Colour data has exactly three channels;
/// alpha must be removed by the caller. Linear inputs use absolute cd/m².
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    width: usize,
    height: usize,
    channels: usize,
    data: Vec<f32>,
}

impl Image {
    /// Validate a buffer and take ownership of it without copying.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidImage`] if either dimension is below 4,
    /// `channels` is not 1 or 3, or `data.len() != width * height * channels`.
    pub fn new(width: usize, height: usize, channels: usize, data: Vec<f32>) -> Result<Self> {
        let len = width
            .checked_mul(height)
            .and_then(|n| n.checked_mul(channels));
        if width < 4 || height < 4 || !matches!(channels, 1 | 3) || len != Some(data.len()) {
            return Err(Error::InvalidImage {
                width,
                height,
                channels,
                len: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            channels,
            data,
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> usize {
        self.width
    }
    /// Height in pixels.
    pub fn height(&self) -> usize {
        self.height
    }
    /// Number of interleaved channels (1 or 3).
    pub fn channels(&self) -> usize {
        self.channels
    }
    /// Interleaved samples.
    pub fn data(&self) -> &[f32] {
        &self.data
    }
    /// Mutable samples; the shape stays fixed.
    pub fn data_mut(&mut self) -> &mut [f32] {
        &mut self.data
    }
    /// Return the underlying buffer.
    pub fn into_data(self) -> Vec<f32> {
        self.data
    }
    pub(crate) fn same_shape(&self, other: &Self) -> bool {
        (self.width, self.height, self.channels) == (other.width, other.height, other.channels)
    }
}

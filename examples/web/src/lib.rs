//! WebAssembly bindings for the browser demo: ColorVideoVDP on a pair of
//! sRGB images read from `ImageData`.

use colorvideovdp::{Color, Cvvdp, DisplayModel, Image, Options};
use wasm_bindgen::prelude::*;

/// The result of one comparison.
#[wasm_bindgen]
pub struct Comparison {
    jod: f32,
    map: Vec<f32>,
}

#[wasm_bindgen]
impl Comparison {
    /// Quality in JOD units: 10 means no visible difference.
    #[wasm_bindgen(getter)]
    pub fn jod(&self) -> f32 {
        self.jod
    }

    /// Raw distortion map, `1 - per_pixel_JOD / 10`, one value per pixel.
    #[wasm_bindgen(getter)]
    pub fn map(&self) -> js_sys::Float32Array {
        js_sys::Float32Array::from(self.map.as_slice())
    }
}

fn rgb(rgba: &[u8], width: usize, height: usize) -> colorvideovdp::Result<Image> {
    let data = rgba
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.0))
        .collect();
    Image::new(width, height, 3, data)
}

/// Compares two images given as RGBA bytes (`ImageData.data`), as seen on the
/// named display, and returns the JOD score and the distortion map. Alpha is
/// ignored; pixels are read as sRGB. Throws an `Error` for invalid input.
#[wasm_bindgen]
pub fn compare(
    reference: &[u8],
    test: &[u8],
    width: u32,
    height: u32,
    display: &str,
) -> Result<Comparison, JsError> {
    let (w, h) = (width as usize, height as usize);
    let expected = w
        .checked_mul(h)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| JsError::new("image too large"))?;
    if reference.len() != expected || test.len() != expected {
        return Err(JsError::new("pixel data does not match the image size"));
    }
    let metric = Cvvdp::new(
        DisplayModel::from_name(display)?,
        Options::default().with_distortion_map(true),
    )?;
    let prediction =
        metric.predict_image(&rgb(test, w, h)?, &rgb(reference, w, h)?, Color::Srgb)?;
    Ok(Comparison {
        jod: prediction.jod,
        map: prediction
            .distortion_map
            .map(|m| m.data)
            .unwrap_or_default(),
    })
}

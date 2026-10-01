use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::error::json_error;
use crate::{Error, Result};

const DISPLAYS: &str = include_str!("../data/display_models.json");
const COLOURS: &str = include_str!("../data/color_spaces.json");

fn embedded_displays() -> Result<&'static BTreeMap<String, DisplayRecord>> {
    static DISPLAY_RECORDS: OnceLock<Result<BTreeMap<String, DisplayRecord>>> = OnceLock::new();
    DISPLAY_RECORDS
        .get_or_init(|| serde_json::from_str(DISPLAYS).map_err(json_error))
        .as_ref()
        .map_err(Clone::clone)
}

fn embedded_color_spaces() -> Result<&'static BTreeMap<String, SpaceRecord>> {
    static SPACE_RECORDS: OnceLock<Result<BTreeMap<String, SpaceRecord>>> = OnceLock::new();
    SPACE_RECORDS
        .get_or_init(|| serde_json::from_str(COLOURS).map_err(json_error))
        .as_ref()
        .map_err(Clone::clone)
}

/// Input colour encoding and primaries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Color {
    /// Use the colour space specified by the display model.
    #[default]
    Display,
    /// sRGB primaries and transfer function; values are in [0, 1].
    Srgb,
    /// BT.709 primaries with absolute linear values in cd/m².
    Linear,
    /// BT.2020 primaries and PQ transfer function; values are in [0, 1].
    Pq,
    /// BT.2020 primaries and HLG transfer function; values are in [0, 1].
    Hlg,
    /// Any exact name from the embedded reference colour-space JSON.
    Named(String),
}

impl Color {
    /// List the embedded reference colour-space names, accepted by [`Color::Named`].
    pub fn names() -> Result<Vec<&'static str>> {
        Ok(embedded_color_spaces()?
            .keys()
            .map(String::as_str)
            .collect())
    }

    pub(crate) fn resolve(&self, display_space: &str) -> Result<ColorSpace> {
        let name = match self {
            Self::Display => display_space,
            Self::Srgb => "sRGB",
            Self::Linear => "BT.709-linear",
            Self::Pq => "BT.2020-PQ",
            Self::Hlg => "BT.2020-HLG",
            Self::Named(name) => name,
        };
        let s = embedded_color_spaces()?
            .get(name)
            .ok_or_else(|| Error::UnknownName(name.into()))?;
        let transfer = match s.eotf.as_str() {
            "sRGB" => TransferFunction::Srgb,
            "linear" => TransferFunction::Linear,
            "PQ" => TransferFunction::Pq,
            "HLG" => TransferFunction::Hlg,
            gamma => TransferFunction::Gamma(
                gamma
                    .parse()
                    .map_err(|_| Error::InvalidConfig("invalid EOTF".into()))?,
            ),
        };
        let rgb_to_xyz = match (s.x, s.y, s.z) {
            (Some(x), Some(y), Some(z)) => Some([x, y, z]),
            _ => None,
        };
        Ok(ColorSpace {
            transfer,
            rgb_to_xyz,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TransferFunction {
    Srgb,
    /// Absolute cd/m², not relative linear RGB.
    Linear,
    Pq,
    /// Includes the display-dependent OOTF.
    Hlg,
    Gamma(f32),
}

#[derive(Deserialize)]
struct SpaceRecord {
    #[serde(rename = "EOTF")]
    eotf: String,
    #[serde(rename = "RGB2X")]
    x: Option<[f32; 3]>,
    #[serde(rename = "RGB2Y")]
    y: Option<[f32; 3]>,
    #[serde(rename = "RGB2Z")]
    z: Option<[f32; 3]>,
}

pub(crate) struct ColorSpace {
    pub transfer: TransferFunction,
    pub rgb_to_xyz: Option<[[f32; 3]; 3]>,
}

/// Display luminance, contrast, ambient light, and exposure.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Photometry {
    /// Peak display luminance in cd/m².
    pub peak_luminance: f64,
    /// Peak to intrinsic black luminance ratio (at least 1).
    pub contrast: f64,
    /// Ambient illuminance in lux.
    pub ambient_illuminance: f64,
    /// Diffuse panel reflectivity, in [0, 1].
    pub reflectivity: f64,
    /// Multiplicative exposure of decoded content.
    pub exposure: f64,
}

impl Photometry {
    /// Construct and validate photometry. Peak plus intrinsic black plus reflected
    /// ambient luminance must not exceed 1,000,000 cd/m²; exposure is at most
    /// 1,000,000. This bounds f32 reductions within the supported numerical domain.
    pub fn new(
        peak_luminance: f64,
        contrast: f64,
        ambient_illuminance: f64,
        reflectivity: f64,
        exposure: f64,
    ) -> Result<Self> {
        let photo = Self {
            peak_luminance,
            contrast,
            ambient_illuminance,
            reflectivity,
            exposure,
        };
        photo.validate()?;
        Ok(photo)
    }

    /// Intrinsic black and reflected ambient luminance, in cd/m².
    pub fn black_level(&self) -> (f64, f64) {
        (
            self.peak_luminance / self.contrast,
            self.ambient_illuminance / std::f64::consts::PI * self.reflectivity,
        )
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if !self.peak_luminance.is_finite()
            || self.peak_luminance < 0.005
            || self.peak_luminance > f32::MAX as f64
            || !self.contrast.is_finite()
            || self.contrast < 1.0
            || !self.ambient_illuminance.is_finite()
            || self.ambient_illuminance < 0.0
            || self.ambient_illuminance > f32::MAX as f64
            || !self.reflectivity.is_finite()
            || !(0.0..=1.0).contains(&self.reflectivity)
            || !self.exposure.is_finite()
            || self.exposure <= 0.0
            || self.exposure > f32::MAX as f64
        {
            return Err(Error::InvalidDisplay("invalid photometry".into()));
        }
        let (black, reflected) = self.black_level();
        if self.peak_luminance + black + reflected > 1_000_000.0 || self.exposure > 1_000_000.0 {
            return Err(Error::InvalidDisplay("photometry exceeds supported numerical domain (luminance or exposure above 1000000)".into()));
        }
        Ok(())
    }
}

/// Display geometry used to determine central pixels per visual degree.
///
/// Image dimensions do not change this value; an image occupies its native
/// pixel size on the display, as in `vvdp_display_geometry.get_ppd`.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    resolution: [usize; 2],
    ppd: f64,
}

impl Geometry {
    /// Construct geometry with explicitly fixed pixels per degree.
    pub fn from_ppd(resolution: [usize; 2], ppd: f64) -> Result<Self> {
        if resolution.contains(&0) || !ppd.is_finite() || ppd <= 0.0 || ppd > f32::MAX as f64 {
            return Err(Error::InvalidDisplay(
                "invalid resolution or pixels per degree".into(),
            ));
        }
        Ok(Self { resolution, ppd })
    }

    /// Construct physical geometry from diagonal size in inches and distance in metres.
    pub fn from_diagonal(
        resolution: [usize; 2],
        diagonal_inches: f64,
        distance_m: f64,
    ) -> Result<Self> {
        if !diagonal_inches.is_finite()
            || diagonal_inches <= 0.0
            || !distance_m.is_finite()
            || distance_m <= 0.0
            || resolution.contains(&0)
        {
            return Err(Error::InvalidDisplay(
                "invalid display size or viewing distance".into(),
            ));
        }
        let ar = resolution[0] as f64 / resolution[1] as f64;
        let height_m = ((diagonal_inches * 25.4).powi(2) / (1.0 + ar.powi(2))).sqrt() / 1000.0;
        let pixel_degrees = 2.0
            * (0.5 * ar * height_m / resolution[0] as f64 / distance_m)
                .atan()
                .to_degrees();
        Self::from_ppd(resolution, 1.0 / pixel_degrees)
    }

    /// Construct geometry from diagonal field of view in degrees (for example a headset).
    pub fn from_fov(resolution: [usize; 2], diagonal_degrees: f64) -> Result<Self> {
        if resolution.contains(&0)
            || !diagonal_degrees.is_finite()
            || !(0.0..180.0).contains(&diagonal_degrees)
            || diagonal_degrees == 0.0
        {
            return Err(Error::InvalidDisplay("invalid field of view".into()));
        }
        let w = resolution[0] as f64;
        let h = resolution[1] as f64;
        let distance_px = w.hypot(h) / (2.0 * (diagonal_degrees * 0.5).to_radians().tan());
        let height_degrees = (h / 2.0 / distance_px).atan().to_degrees() * 2.0;
        let height_m = 2.0 * (height_degrees / 2.0).to_radians().tan() * 3.0;
        let pixel_degrees = 2.0 * (0.5 * height_m * (w / h) / w / 3.0).atan().to_degrees();
        Self::from_ppd(resolution, 1.0 / pixel_degrees)
    }

    /// Central pixels per visual degree.
    pub fn pixels_per_degree(&self) -> f64 {
        self.ppd
    }
    /// Native [width, height] display resolution.
    pub fn resolution(&self) -> [usize; 2] {
        self.resolution
    }
}

/// A photometric and geometric display model.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DisplayModel {
    /// Reference display identifier, or a descriptive custom name.
    pub name: String,
    /// Display photometry.
    pub photometry: Photometry,
    /// Display geometry.
    pub geometry: Geometry,
    /// Default input colour-space name, used by [`Color::Display`].
    pub color_space: String,
}

impl DisplayModel {
    /// Construct and validate a custom display.
    pub fn new(
        name: impl Into<String>,
        photometry: Photometry,
        geometry: Geometry,
        color_space: impl Into<String>,
    ) -> Result<Self> {
        let model = Self {
            name: name.into(),
            photometry,
            geometry,
            color_space: color_space.into(),
        };
        model.validate()?;
        Ok(model)
    }

    /// Load a display from the embedded reference `display_models.json`,
    /// for example `"standard_4k"` or `"standard_hdr_pq"`.
    pub fn from_name(name: &str) -> Result<Self> {
        let record = embedded_displays()?
            .get(name)
            .ok_or_else(|| Error::UnknownName(name.into()))?;
        Self::from_record(name, record)
    }

    /// List all embedded display identifiers.
    pub fn names() -> Result<Vec<&'static str>> {
        Ok(embedded_displays()?.keys().map(String::as_str).collect())
    }

    /// Load a named display from JSON in the reference `display_models.json` format.
    pub fn from_json(name: &str, json: &str) -> Result<Self> {
        let models: BTreeMap<String, DisplayRecord> =
            serde_json::from_str(json).map_err(json_error)?;
        let record = models
            .get(name)
            .ok_or_else(|| Error::UnknownName(name.into()))?;
        Self::from_record(name, record)
    }

    // display_model.py: vvdp_display_photometry.load and vvdp_display_geometry.
    fn from_record(name: &str, r: &DisplayRecord) -> Result<Self> {
        let contrast = if let Some(min) = r.min_luminance {
            if !min.is_finite() || min <= 0.0 {
                return Err(Error::InvalidDisplay("invalid minimum luminance".into()));
            }
            r.max_luminance / min
        } else {
            r.contrast.unwrap_or(500.0)
        };
        let geometry = if let Some(ppd) = r.pixels_per_degree {
            Geometry::from_ppd(r.resolution, ppd)?
        } else if let Some(fov) = r.fov_diagonal {
            Geometry::from_fov(r.resolution, fov)?
        } else {
            let distance = r
                .viewing_distance_meters
                .or(r.viewing_distance_inches.map(|v| v * 0.0254))
                .ok_or_else(|| Error::InvalidDisplay("missing viewing distance".into()))?;
            let diagonal = r
                .diagonal_size_meters
                .map(|v| v / 0.0254)
                .or(r.diagonal_size_inches)
                .ok_or_else(|| Error::InvalidDisplay("missing diagonal size".into()))?;
            Geometry::from_diagonal(r.resolution, diagonal, distance)?
        };
        let model = Self {
            name: name.into(),
            geometry,
            color_space: r.colorspace.clone().unwrap_or_else(|| "sRGB".into()),
            photometry: Photometry {
                peak_luminance: r.max_luminance,
                contrast,
                ambient_illuminance: r.ambient.unwrap_or(0.0),
                reflectivity: r.k_refl.unwrap_or(0.005),
                exposure: r.exposure.unwrap_or(1.0),
            },
        };
        model.validate()?;
        Ok(model)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        self.photometry.validate()?;
        Color::Display.resolve(&self.color_space)?;
        Ok(())
    }
}

#[derive(Deserialize)]
struct DisplayRecord {
    resolution: [usize; 2],
    max_luminance: f64,
    min_luminance: Option<f64>,
    contrast: Option<f64>,
    #[serde(rename = "E_ambient")]
    ambient: Option<f64>,
    k_refl: Option<f64>,
    exposure: Option<f64>,
    colorspace: Option<String>,
    pixels_per_degree: Option<f64>,
    viewing_distance_meters: Option<f64>,
    viewing_distance_inches: Option<f64>,
    diagonal_size_meters: Option<f64>,
    diagonal_size_inches: Option<f64>,
    fov_diagonal: Option<f64>,
}

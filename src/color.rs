use crate::allocation::collect;
use crate::display::{ColorSpace, Photometry, TransferFunction};
use crate::plane::Plane;
use crate::{Error, Image, Result};

const XYZ_LMS: [[f64; 3]; 3] = [
    [0.187596268556126, 0.585168649077728, -0.026384263306304],
    [-0.133397430663221, 0.405505777260049, 0.034502127690364],
    [0.000244379021663, -0.000542995890619, 0.019406849066323],
];
const LMS_DKL: [[f64; 3]; 3] = [
    [1.0, 1.0, 0.0],
    [1.0, -2.311130179947035, 0.0],
    [-1.0, -1.0, 50.977_571_328_718_78],
];

fn matmul(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    std::array::from_fn(|r| {
        std::array::from_fn(|c| (a[r][0] * b[0][c] + a[r][1] * b[1][c]) + a[r][2] * b[2][c])
    })
}

// display_model.py: vvdp_display_photo_eotf.forward and linear_2_target_colorspace.
pub(crate) fn transform(
    image: &Image,
    photo: &Photometry,
    space: &ColorSpace,
    parallel: bool,
) -> Result<Vec<Plane>> {
    if image.channels() == 3 && space.rgb_to_xyz.is_none() {
        return Err(Error::ColorChannelMismatch);
    }
    if matches!(space.transfer, TransferFunction::Hlg)
        && photo.peak_luminance > 1000.0
        && photo.ambient_illuminance == 0.0
    {
        return Err(Error::InvalidDisplay(
            "HLG above 1000 cd/m² requires positive ambient illuminance".into(),
        ));
    }
    let (black, reflected) = photo.black_level();
    let scale = (photo.peak_luminance - black) as f32;
    let peak = photo.peak_luminance as f32;
    let exposure = photo.exposure as f32;
    let gamma = if photo.peak_luminance > 1000.0 {
        (1.2 + 0.42 * (photo.peak_luminance / 1000.0).log10()
            - 0.07623 * (photo.ambient_illuminance / 5.0).log10()) as f32
    } else {
        1.2
    };
    let matrix = space.rgb_to_xyz.map(|rgb| {
        matmul(
            matmul(
                LMS_DKL.map(|r| r.map(|x| x as f32)),
                XYZ_LMS.map(|r| r.map(|x| x as f32)),
            ),
            rgb,
        )
    });
    let linear: Vec<Plane> = collect((0..image.channels()).map(|channel| {
        Plane::generate(image.width(), image.height(), parallel, |x, y| {
            let i = (y * image.width() + x) * image.channels();
            let v = image.data()[i + channel];
            let v = if matches!(space.transfer, TransferFunction::Linear) {
                v
            } else {
                v.clamp(0.0, 1.0)
            };
            let relative = match space.transfer {
                TransferFunction::Srgb => {
                    if v > 0.04045 {
                        ((v + 0.055) / 1.055).powf(2.4)
                    } else {
                        v / 12.92
                    }
                }
                TransferFunction::Gamma(g) => v.powf(g),
                TransferFunction::Hlg => {
                    let rgb: [f32; 3] = std::array::from_fn(|c| {
                        hlg_inverse(image.data()[i + c.min(image.channels() - 1)].clamp(0.0, 1.0))
                    });
                    let luminance = (0.2627 * rgb[0] + 0.6780 * rgb[1]) + 0.0593 * rgb[2];
                    luminance.powf(gamma - 1.0) * rgb[channel]
                }
                TransferFunction::Linear => {
                    return (v * exposure).clamp((black as f32).max(0.005), peak) + reflected as f32
                }
                TransferFunction::Pq => {
                    let t = v.powf(1.0 / 78.84375);
                    let l = 10000.0
                        * ((t - 0.8359375).max(0.0) / (18.851563 - 18.6875 * t))
                            .powf(1.0 / 0.15930176);
                    return (l * exposure).clamp(0.005, peak) + black as f32 + reflected as f32;
                }
            };
            let relative =
                if exposure == 1.0 && !matches!(space.transfer, TransferFunction::Gamma(_)) {
                    relative
                } else {
                    (relative * exposure).clamp(0.0, 1.0)
                };
            scale * relative + black as f32 + reflected as f32
        })
    }))?;
    let result = if let Some(m) = matrix.filter(|_| image.channels() == 3) {
        collect((0..3).map(|c| {
            Plane::generate(image.width(), image.height(), parallel, |x, y| {
                let i = y * image.width() + x;
                (linear[0].data[i] * m[c][0] + linear[1].data[i] * m[c][1])
                    + linear[2].data[i] * m[c][2]
            })
        }))?
    } else {
        collect((0..3).map(|_| {
            Plane::generate(image.width(), image.height(), parallel, |x, y| {
                linear[0].data[y * image.width() + x]
            })
        }))?
    };
    if result.iter().any(|p| p.data.iter().any(|v| !v.is_finite())) {
        return Err(Error::NumericalFailure);
    }
    Ok(result)
}

fn hlg_inverse(v: f32) -> f32 {
    let a = 0.17883277_f64;
    let b = (1.0 - 4.0 * a) as f32;
    let c = (0.5 - a * (4.0 * a).ln()) as f32;
    if v <= 0.5 {
        v.powi(2) / 3.0
    } else {
        (((v - c) / a as f32).exp() + b) / 12.0
    }
}

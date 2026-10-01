use colorvideovdp::{
    Color, Cvvdp, DisplayModel, Execution, Geometry, Image, Options, Photometry, TemporalPadding,
};
use std::sync::OnceLock;

fn byte(data: &[u8], index: usize) -> u8 {
    data.get(index).copied().unwrap_or(0)
}
fn bits(data: &[u8], index: usize) -> u32 {
    u32::from_le_bytes(std::array::from_fn(|i| byte(data, index + i)))
}
fn metric() -> Option<&'static Cvvdp> {
    static METRIC: OnceLock<Option<Cvvdp>> = OnceLock::new();
    METRIC
        .get_or_init(|| {
            DisplayModel::from_name("standard_4k")
                .and_then(|d| {
                    Cvvdp::new(
                        d,
                        Options::default()
                            .with_distortion_map(true)
                            .with_temporal_padding(TemporalPadding::Symmetric)
                            .with_execution(Execution::SingleThread)
                            .with_memory_limit_bytes(4 * 1024 * 1024),
                    )
                })
                .ok()
        })
        .as_ref()
}
fn image(data: &[u8], offset: usize) -> colorvideovdp::Result<Image> {
    let mode = byte(data, 0) % 16;
    let w = if mode == 0 {
        usize::MAX
    } else {
        (byte(data, 1) % 13) as usize
    };
    let h = (byte(data, 2) % 13) as usize;
    let c = [1, 3, 0, 4][(byte(data, 3) % 4) as usize];
    let n = if mode == 0 { 1 } else { w * h * c };
    let values = (0..n)
        .map(|i| {
            if mode < 4 {
                f32::from_bits(bits(data, offset + i * 4))
            } else {
                byte(data, offset + i) as f32 / 255.0
            }
        })
        .collect();
    Image::new(w, h, c, values)
}
fn color(data: &[u8]) -> Color {
    match byte(data, 4) % 7 {
        0 => Color::Srgb,
        1 => Color::Linear,
        2 => Color::Pq,
        3 => Color::Hlg,
        4 => Color::Named("luminance".into()),
        5 => Color::Named(String::from_utf8_lossy(data).into_owned()),
        _ => Color::Display,
    }
}
pub fn image_predict(data: &[u8]) -> bool {
    let (Ok(test), Ok(reference), Some(metric)) = (image(data, 8), image(data, 16), metric())
    else {
        return false;
    };
    metric.predict_image(&test, &reference, color(data)).is_ok()
}
pub fn video_predict(data: &[u8]) -> bool {
    let Some(metric) = metric() else {
        return false;
    };
    let n = (byte(data, 5) % 5) as usize;
    let test = (0..n)
        .map(|i| image(data, 8 + i * 4))
        .collect::<colorvideovdp::Result<Vec<_>>>();
    let reference = (0..n)
        .map(|i| image(data, 16 + i * 4))
        .collect::<colorvideovdp::Result<Vec<_>>>();
    let fps = match byte(data, 6) % 8 {
        0 => 0.0,
        1 => -1.0,
        2 => f32::NAN,
        3 => f32::INFINITY,
        4 => f32::from_bits(bits(data, 7)),
        5 => 30.0,
        6 => 120.0,
        _ => 16384.0,
    };
    let (Ok(test), Ok(reference)) = (test, reference) else {
        return false;
    };
    metric
        .predict_video(&test, &reference, fps, color(data))
        .is_ok()
}
pub fn display_parse(data: &[u8]) -> bool {
    let raw = String::from_utf8_lossy(data);
    let mut accepted = DisplayModel::from_json("fuzz", &raw).is_ok();
    let value = |i| {
        f64::from_bits(u64::from_le_bytes(std::array::from_fn(|j| {
            byte(data, i + j)
        })))
    };
    let record = format!(
        r#"{{"fuzz":{{"resolution":[{},{}],"pixels_per_degree":{},"max_luminance":{},"contrast":{},"E_ambient":{},"k_refl":{},"exposure":{}}}}}"#,
        bits(data, 0),
        bits(data, 4),
        value(8),
        value(16),
        value(24),
        value(32),
        value(40),
        value(48)
    );
    accepted |= DisplayModel::from_json("fuzz", &record).is_ok();
    let _ = Photometry::new(value(16), value(24), value(32), value(40), value(48));
    let _ = Geometry::from_diagonal(
        [bits(data, 0) as usize, bits(data, 4) as usize],
        value(8),
        value(16),
    );
    let _ = Geometry::from_fov([bits(data, 0) as usize, bits(data, 4) as usize], value(24));
    accepted
}

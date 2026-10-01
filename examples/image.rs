//! Predict the quality of a slightly distorted image and request its distortion map.

use colorvideovdp::{Color, Cvvdp, DisplayModel, Image, Options};

fn main() -> colorvideovdp::Result<()> {
    let reference = Image::new(64, 64, 3, vec![0.5; 64 * 64 * 3])?;
    let mut test = reference.clone();
    for (i, value) in test.data_mut().iter_mut().enumerate() {
        *value += (i % 17) as f32 * 0.001;
    }

    let display = DisplayModel::from_name("standard_4k")?;
    let metric = Cvvdp::new(display, Options::default().with_distortion_map(true))?;
    let prediction = metric.predict_image(&test, &reference, Color::Srgb)?;

    println!("Quality: {:.4} JOD", prediction.jod);
    if let Some(map) = prediction.distortion_map {
        println!(
            "Distortion map: {} × {} × {}",
            map.width, map.height, map.frames
        );
    }
    Ok(())
}

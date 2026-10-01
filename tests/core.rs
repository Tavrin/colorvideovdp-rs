//! Input contracts and execution-mode tests.

use colorvideovdp::{
    Color, Cvvdp, DisplayModel, Error, Execution, Geometry, Image, Options, Result, TemporalPadding,
};

fn single_thread() -> Options {
    Options::default().with_execution(Execution::SingleThread)
}

fn metric(options: Options) -> Result<Cvvdp> {
    Cvvdp::new(DisplayModel::from_name("standard_4k")?, options)
}

fn solid(width: usize, height: usize, value: f32) -> Result<Image> {
    Image::new(width, height, 3, vec![value; width * height * 3])
}

#[test]
fn identity_and_map_shape() -> Result<()> {
    let image = solid(17, 16, 0.4)?;
    let result = metric(single_thread().with_distortion_map(true))?.predict_image(
        &image,
        &image,
        Color::Srgb,
    )?;
    assert_eq!(result.jod, 10.0);
    assert_eq!(result.channels, 3);
    assert_eq!(result.frames, 1);
    assert_eq!(result.band_frequencies.last(), Some(&0.1));
    assert!(result.quality_per_channel.iter().all(|&x| x == 0.0));
    let map = result.distortion_map.ok_or(Error::NumericalFailure)?;
    assert_eq!(
        (map.width, map.height, map.frames, map.data.len()),
        (17, 16, 1, 272)
    );
    assert!(map.data.iter().all(|&x| x == 0.0));
    Ok(())
}

#[test]
fn validation_rejects_bad_shapes_and_non_finite_values() -> Result<()> {
    assert!(Image::new(0, 4, 3, vec![]).is_err());
    assert_eq!(
        Image::new(3, 4, 3, vec![0.0; 36]).err(),
        Some(Error::InvalidImage {
            width: 3,
            height: 4,
            channels: 3,
            len: 36
        })
    );
    assert!(Image::new(4, 4, 4, vec![0.0; 64]).is_err());
    assert!(Image::new(4, 4, 3, vec![0.0; 47]).is_err());
    assert!(Image::new(usize::MAX, 4, 3, vec![0.0]).is_err());
    let good = solid(8, 8, 0.5)?;
    let other = solid(9, 8, 0.5)?;
    let m = metric(single_thread())?;
    assert_eq!(
        m.predict_image(&good, &other, Color::Srgb).err(),
        Some(Error::ShapeMismatch)
    );
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let bad = solid(8, 8, value)?;
        assert_eq!(
            m.predict_image(&bad, &good, Color::Srgb).err(),
            Some(Error::NonFiniteInput)
        );
    }
    for fps in [0.0, -1.0, f32::NAN, f32::INFINITY, 16385.0] {
        assert!(matches!(
            m.predict_video(
                std::slice::from_ref(&good),
                std::slice::from_ref(&good),
                fps,
                Color::Srgb
            ),
            Err(Error::InvalidFrameRate(_))
        ));
    }
    assert_eq!(
        m.predict_video(&[], &[], 30.0, Color::Srgb).err(),
        Some(Error::EmptySequence)
    );
    assert_eq!(
        m.predict_video(
            &[good.clone(), good.clone()],
            std::slice::from_ref(&good),
            30.0,
            Color::Srgb
        )
        .err(),
        Some(Error::ShapeMismatch)
    );
    assert_eq!(
        m.predict_video(
            &[good.clone(), other],
            &[good.clone(), good],
            30.0,
            Color::Srgb
        )
        .err(),
        Some(Error::ShapeMismatch)
    );
    Ok(())
}

#[test]
fn displays_and_color_spaces_load_from_embedded_data() -> Result<()> {
    let names = DisplayModel::names()?;
    assert_eq!(names.len(), 26);
    for name in names {
        let display = DisplayModel::from_name(name)?;
        assert!(display.geometry.pixels_per_degree() > 0.0);
        Cvvdp::new(display, single_thread())?;
    }
    let spaces = Color::names()?;
    assert_eq!(spaces.len(), 22);
    assert!(spaces.contains(&"BT.2020-HLG"));
    assert_eq!(
        DisplayModel::from_name("absent").err(),
        Some(Error::UnknownName("absent".into()))
    );
    let image = solid(8, 8, 0.5)?;
    let m = metric(single_thread())?;
    assert!(m
        .predict_image(&image, &image, Color::Named("absent".into()))
        .is_err());
    assert_eq!(
        m.predict_image(&image, &image, Color::Named("luminance".into()))
            .err(),
        Some(Error::ColorChannelMismatch)
    );
    Ok(())
}

#[test]
fn custom_geometry_and_photometry_validation() -> Result<()> {
    assert!(Geometry::from_ppd([1920, 1080], f64::NAN).is_err());
    assert!(Geometry::from_fov([1920, 1080], 180.0).is_err());
    assert!(Geometry::from_diagonal([1920, 1080], 24.0, 0.0).is_err());
    let geometry = Geometry::from_diagonal([3840, 2160], 30.0, 0.7472)?;
    assert!((geometry.pixels_per_degree() - 75.402449345163).abs() < 0.001);
    let custom = r#"{"monitor": {"resolution": [800, 600], "pixels_per_degree": 45, "max_luminance": 200, "min_luminance": 0.2}}"#;
    let mut display = DisplayModel::from_json("monitor", custom)?;
    assert_eq!(display.geometry.pixels_per_degree(), 45.0);
    assert_eq!(display.photometry.black_level(), (0.2, 0.0));
    display.photometry.contrast = 0.0;
    assert!(Cvvdp::new(display, Options::default()).is_err());
    Ok(())
}

#[test]
fn single_frame_video_is_an_image() -> Result<()> {
    let reference = solid(8, 8, 0.5)?;
    let test = solid(8, 8, 0.55)?;
    let metric = metric(single_thread())?;
    let image = metric.predict_image(&test, &reference, Color::Srgb)?;
    let video = metric.predict_video(&[test], &[reference], 30.0, Color::Srgb)?;
    assert_eq!(image.jod, video.jod);
    assert_eq!(video.channels, 3);
    assert!(video.distortion_map.is_none());
    Ok(())
}

#[test]
fn video_padding_and_identity() -> Result<()> {
    let frames: Vec<_> = [0.2, 0.5, 0.4]
        .into_iter()
        .map(|v| solid(8, 9, v))
        .collect::<Result<_>>()?;
    for padding in [TemporalPadding::Replicate, TemporalPadding::Symmetric] {
        let metric = metric(
            single_thread()
                .with_distortion_map(true)
                .with_temporal_padding(padding),
        )?;
        let prediction = metric.predict_video(&frames, &frames, 120.0, Color::Srgb)?;
        assert_eq!(prediction.jod, 10.0);
        assert_eq!(prediction.channels, 4);
        assert_eq!(
            prediction
                .distortion_map
                .ok_or(Error::NumericalFailure)?
                .data
                .len(),
            8 * 9 * 3
        );
    }
    Ok(())
}

#[cfg(feature = "parallel")]
#[test]
fn parallel_and_single_predictions_are_identical() -> Result<()> {
    let reference = solid(80, 65, 0.5)?;
    let data = (0..80 * 65 * 3)
        .map(|i| 0.4 + (i % 37) as f32 / 100.0)
        .collect();
    let test = Image::new(80, 65, 3, data)?;
    let a = metric(single_thread().with_distortion_map(true))?.predict_image(
        &test,
        &reference,
        Color::Srgb,
    )?;
    let b = metric(
        Options::default()
            .with_distortion_map(true)
            .with_execution(Execution::Parallel),
    )?
    .predict_image(&test, &reference, Color::Srgb)?;
    assert_eq!(a, b);
    Ok(())
}

#[cfg(not(feature = "parallel"))]
#[test]
fn requesting_unavailable_parallel_execution_returns_an_error() {
    assert_eq!(
        DisplayModel::from_name("standard_4k")
            .and_then(|d| Cvvdp::new(d, Options::default().with_execution(Execution::Parallel)))
            .err(),
        Some(Error::ParallelUnavailable)
    );
}

#[test]
fn prediction_memory_limit_rejects_before_allocating_working_planes() -> Result<()> {
    let image = solid(4, 4, 0.5)?;
    let m = metric(single_thread().with_memory_limit_bytes(1))?;
    assert!(matches!(
        m.predict_image(&image, &image, Color::Srgb),
        Err(Error::MemoryLimitExceeded { limit: 1, .. })
    ));
    Ok(())
}

#[test]
fn extreme_photometry_is_explicitly_outside_supported_domain() -> Result<()> {
    use colorvideovdp::Photometry;
    assert!(matches!(
        Photometry::new(1e38, 10000.0, 0.0, 0.005, 1.0),
        Err(Error::InvalidDisplay(_))
    ));
    assert!(Photometry::new(100.0, 100.0, 1e20, 0.5, 1.0).is_err());
    assert!(Photometry::new(100.0, 100.0, 0.0, 0.005, 1e20).is_err());
    let geometry = Geometry::from_ppd([1920, 1080], 45.0)?;
    let photo = Photometry::new(999_000.0, 1000.0, 0.0, 0.005, 1.0)?;
    let display = DisplayModel::new("bright", photo, geometry, "sRGB")?;
    let image = solid(4, 4, 1.0)?;
    assert_eq!(
        Cvvdp::new(display, single_thread())?
            .predict_image(&image, &image, Color::Srgb)?
            .jod,
        10.0
    );
    Ok(())
}

#[test]
fn maximum_frame_rate_tiny_video_is_bounded_and_finite() -> Result<()> {
    let frames = vec![solid(4, 4, 0.25)?, solid(4, 4, 0.75)?];
    let m = metric(single_thread().with_temporal_padding(TemporalPadding::Symmetric))?;
    let result = m.predict_video(&frames, &frames, 16384.0, Color::Srgb)?;
    assert_eq!(result.jod, 10.0);
    assert!(result.quality_per_channel.iter().all(|x| x.is_finite()));
    Ok(())
}

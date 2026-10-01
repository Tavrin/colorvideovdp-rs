//! Regression checks against persisted CPU reference predictions.

use std::path::Path;

use colorvideovdp::{Color, Cvvdp, DisplayModel, Execution, Image, Options, TemporalPadding};
use serde::Deserialize;

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    display: String,
    color: String,
    width: usize,
    height: usize,
    channels: usize,
    frames: usize,
    fps: f32,
    padding: String,
    test: String,
    reference: String,
    heatmap: String,
    features: String,
    jod: f32,
    custom_display: Option<serde_json::Value>,
}

fn raw(path: &Path) -> std::io::Result<Vec<f32>> {
    Ok(std::fs::read(path)?
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

#[test]
fn persisted_cpu_oracle_regressions() -> Result<(), Box<dyn std::error::Error>> {
    let corpus: Corpus = serde_json::from_str(include_str!("data/oracle.json"))?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    for case in corpus.cases {
        let options = Options::default()
            .with_distortion_map(true)
            .with_execution(Execution::SingleThread)
            .with_temporal_padding(if case.padding == "symmetric" {
                TemporalPadding::Symmetric
            } else {
                TemporalPadding::Replicate
            });
        let display = if let Some(json) = &case.custom_display {
            DisplayModel::from_json(&case.display, &serde_json::to_string(json)?)?
        } else {
            DisplayModel::from_name(&case.display)?
        };
        let metric = Cvvdp::new(display, options)?;
        let length = case.width * case.height * case.channels;
        let make_frames = |filename: &str| -> Result<Vec<Image>, Box<dyn std::error::Error>> {
            let values = raw(&root.join(filename))?;
            assert_eq!(values.len(), length * case.frames);
            values
                .chunks_exact(length)
                .map(|p| {
                    Ok(Image::new(
                        case.width,
                        case.height,
                        case.channels,
                        p.to_vec(),
                    )?)
                })
                .collect()
        };
        let test = make_frames(&case.test)?;
        let reference = make_frames(&case.reference)?;
        let color = Color::Named(case.color);
        let prediction = if case.frames == 1 {
            metric.predict_image(&test[0], &reference[0], color)?
        } else {
            metric.predict_video(&test, &reference, case.fps, color)?
        };
        assert!(
            (prediction.jod - case.jod).abs() <= 0.01,
            "{} JOD",
            case.name
        );
        let map = prediction.distortion_map.ok_or("missing map")?;
        let expected = raw(&root.join(case.heatmap))?;
        assert_eq!(map.data.len(), expected.len());
        assert!(
            map.data
                .iter()
                .zip(expected)
                .all(|(&a, b)| (a - b).abs() <= 0.001),
            "{} heatmap",
            case.name
        );
        let expected = raw(&root.join(case.features))?;
        assert_eq!(prediction.quality_per_channel.len(), expected.len());
        assert!(
            prediction
                .quality_per_channel
                .iter()
                .zip(expected)
                .all(|(&a, b)| (a - b).abs() <= 0.001),
            "{} features",
            case.name
        );
    }
    Ok(())
}

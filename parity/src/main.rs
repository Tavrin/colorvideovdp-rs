use std::error::Error;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Instant;

use colorvideovdp::{Color, Cvvdp, DisplayModel, Execution, Image, Options, TemporalPadding};
use serde::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Deserialize)]
struct Manifest {
    jod_tolerance: f32,
    map_tolerance: f32,
    map_f32_tolerance: f32,
    feature_tolerance: f32,
    #[serde(default)]
    quantization_aware: bool,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    group: String,
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
    heatmap_f32: String,
    features: String,
    jod: f32,
    band_frequencies: Vec<f32>,
    custom_display: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct Comparison {
    name: String,
    group: String,
    jod_reference: f32,
    jod_rust: f32,
    jod_error: f32,
    map_error: f32,
    map_f32_error: f32,
    reference_quantization_error: f32,
    feature_error: f32,
    band_frequency_error: f32,
}

fn read_raw(path: &Path) -> Result<Vec<f32>> {
    let bytes = fs::read(path)?;
    if bytes.len() % 4 != 0 {
        return Err("raw file length is not a multiple of four".into());
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

fn images(path: &Path, case: &Case) -> Result<Vec<Image>> {
    let data = read_raw(path)?;
    let frame_length = case.width * case.height * case.channels;
    if data.len() != frame_length * case.frames {
        return Err("input file shape mismatch".into());
    }
    data.chunks_exact(frame_length)
        .map(|frame| {
            Ok(Image::new(
                case.width,
                case.height,
                case.channels,
                frame.to_vec(),
            )?)
        })
        .collect()
}

fn max_error(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.len() != b.len() || a.iter().chain(b).any(|v| !v.is_finite()) {
        return Err("invalid or non-finite comparison buffers".into());
    }
    Ok(a.iter()
        .zip(b)
        .map(|(&a, &b)| (a - b).abs())
        .fold(0.0, f32::max))
}

fn evaluator(case: &Case, execution: Execution, heatmap: bool) -> Result<Cvvdp> {
    let padding = match case.padding.as_str() {
        "replicate" => TemporalPadding::Replicate,
        "symmetric" => TemporalPadding::Symmetric,
        _ => return Err("invalid padding".into()),
    };
    let options = Options::default()
        .with_distortion_map(heatmap)
        .with_temporal_padding(padding)
        .with_execution(execution)
        .with_memory_limit_bytes(if heatmap {
            1024 * 1024 * 1024
        } else {
            2 * 1024 * 1024 * 1024
        });
    let display = if let Some(custom) = &case.custom_display {
        DisplayModel::from_json(&case.display, &serde_json::to_string(custom)?)?
    } else {
        DisplayModel::from_name(&case.display)?
    };
    Ok(Cvvdp::new(display, options)?)
}

fn predict(
    metric: &Cvvdp,
    case: &Case,
    test: &[Image],
    reference: &[Image],
) -> Result<colorvideovdp::Prediction> {
    let color = Color::Named(case.color.clone());
    Ok(if case.frames == 1 {
        metric.predict_image(&test[0], &reference[0], color)?
    } else {
        metric.predict_video(test, reference, case.fps, color)?
    })
}

fn save_prediction(path: &Path, result: &colorvideovdp::Prediction) -> Result<()> {
    let mut out = BufWriter::new(fs::File::create(path)?);
    out.write_all(&result.jod.to_bits().to_le_bytes())?;
    for n in [result.channels, result.frames] {
        out.write_all(&(n as u64).to_le_bytes())?;
    }
    for values in [&result.band_frequencies, &result.quality_per_channel] {
        out.write_all(&(values.len() as u64).to_le_bytes())?;
        for x in values {
            out.write_all(&x.to_bits().to_le_bytes())?;
        }
    }
    if let Some(map) = &result.distortion_map {
        out.write_all(&[1])?;
        for n in [map.width, map.height, map.frames, map.data.len()] {
            out.write_all(&(n as u64).to_le_bytes())?;
        }
        for x in &map.data {
            out.write_all(&x.to_bits().to_le_bytes())?;
        }
    } else {
        out.write_all(&[0])?;
    }
    out.flush()?;
    Ok(())
}

fn compare(directory: &Path, execution: Execution) -> Result<()> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
    if manifest.cases.is_empty() {
        return Err("empty parity corpus".into());
    }
    let mut results = Vec::new();
    let mut failures = 0;
    for case in &manifest.cases {
        let test = images(&directory.join(&case.test), case)?;
        let reference = images(&directory.join(&case.reference), case)?;
        let result = predict(&evaluator(case, execution, true)?, case, &test, &reference)?;
        let suffix = if execution == Execution::SingleThread {
            "single"
        } else {
            "parallel"
        };
        let output = directory.join(format!("{}-prediction-{suffix}.bin", case.name));
        save_prediction(&output, &result)?;
        if execution == Execution::Parallel
            && fs::read(&output)?
                != fs::read(directory.join(format!("{}-prediction-single.bin", case.name)))?
        {
            return Err(format!("full parallel prediction differs: {}", case.name).into());
        }
        let map = result
            .distortion_map
            .as_ref()
            .ok_or("missing distortion map")?;
        if (map.width, map.height, map.frames) != (case.width, case.height, case.frames) {
            return Err("map shape mismatch".into());
        }
        let expected = read_raw(&directory.join(&case.heatmap))?;
        let expected_f32 = read_raw(&directory.join(&case.heatmap_f32))?;
        let row = Comparison {
            name: case.name.clone(),
            group: case.group.clone(),
            jod_reference: case.jod,
            jod_rust: result.jod,
            jod_error: (result.jod - case.jod).abs(),
            map_error: max_error(&map.data, &expected)?,
            map_f32_error: max_error(&map.data, &expected_f32)?,
            reference_quantization_error: max_error(&expected, &expected_f32)?,
            band_frequency_error: max_error(&result.band_frequencies, &case.band_frequencies)?,
            feature_error: max_error(
                &result.quality_per_channel,
                &read_raw(&directory.join(&case.features))?,
            )?,
        };
        let map_limit = if manifest.quantization_aware {
            manifest
                .map_tolerance
                .max(row.reference_quantization_error + manifest.map_f32_tolerance)
        } else {
            manifest.map_tolerance
        };
        if !row.jod_error.is_finite()
            || row.jod_error > manifest.jod_tolerance
            || row.map_error > map_limit
            || row.map_f32_error > manifest.map_f32_tolerance
            || row.feature_error > manifest.feature_tolerance
            || row.band_frequency_error > 0.0001
        {
            failures += 1;
            eprintln!(
                "FAIL {}: JOD error {:.8}, map error {:.8}, unquantized {:.8}",
                row.name, row.jod_error, row.map_error, row.map_f32_error
            );
        }
        results.push(row);
    }
    let file = if execution == Execution::SingleThread {
        "comparison-single.json"
    } else {
        "comparison-parallel.json"
    };
    fs::write(
        directory.join(file),
        serde_json::to_string_pretty(&results)?,
    )?;
    println!(
        "{:?}: {} cases, {} failures; max |ΔJOD|={:.8}, max |Δmap|={:.8}, max |Δmap f32|={:.8}",
        execution,
        results.len(),
        failures,
        results.iter().map(|r| r.jod_error).fold(0.0, f32::max),
        results.iter().map(|r| r.map_error).fold(0.0, f32::max),
        results.iter().map(|r| r.map_f32_error).fold(0.0, f32::max)
    );
    if failures != 0 {
        return Err(format!("{failures} parity cases failed").into());
    }
    Ok(())
}

#[derive(Serialize)]
struct Timing {
    name: String,
    execution: String,
    milliseconds: f64,
    samples_ms: Vec<f64>,
    jod: f32,
}

fn benchmark(directory: &Path, execution: Execution, repeats: usize) -> Result<()> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
    let mut timings = Vec::new();
    for case in &manifest.cases {
        let test = images(&directory.join(&case.test), case)?;
        let reference = images(&directory.join(&case.reference), case)?;
        let metric = evaluator(case, execution, false)?;
        let warmup = predict(&metric, case, &test, &reference)?;
        let mut samples = Vec::new();
        for _ in 0..repeats {
            let start = Instant::now();
            let result = std::hint::black_box(predict(&metric, case, &test, &reference)?);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            if (result.jod - case.jod).abs() > manifest.jod_tolerance {
                return Err("benchmark parity failed".into());
            }
        }
        let mut sorted = samples.clone();
        sorted.sort_by(f64::total_cmp);
        let median = sorted[sorted.len() / 2];
        println!("{} {:?}: {:.2} ms", case.name, execution, median);
        timings.push(Timing {
            name: case.name.clone(),
            execution: format!("{execution:?}"),
            milliseconds: median,
            samples_ms: samples,
            jod: warmup.jod,
        });
    }
    fs::write(
        directory.join(format!("benchmark-rust-{execution:?}.json")),
        serde_json::to_string_pretty(&timings)?,
    )?;
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let action = args
        .get(1)
        .map(String::as_str)
        .ok_or("expected compare or benchmark")?;
    let directory = Path::new(args.get(2).ok_or("expected corpus directory")?);
    let execution = match args.get(3).map(String::as_str).unwrap_or("single") {
        "single" => Execution::SingleThread,
        "parallel" => Execution::Parallel,
        _ => return Err("expected single or parallel".into()),
    };
    #[cfg(feature = "parallel")]
    if execution == Execution::Parallel {
        let threads: usize = std::env::var("RAYON_NUM_THREADS")
            .unwrap_or_else(|_| "16".into())
            .parse()?;
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()?;
    }
    match action {
        "compare" => compare(directory, execution),
        "benchmark" => benchmark(
            directory,
            execution,
            args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(3),
        ),
        _ => Err("expected compare or benchmark".into()),
    }
}

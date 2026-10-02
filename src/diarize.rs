use std::collections::BTreeMap;
use std::path::Path;

use rubato::{FftFixedIn, Resampler};
use speakrs::{ExecutionMode, OwnedDiarizationPipeline};

const TARGET_SAMPLE_RATE: u32 = 16_000;
const MIN_DURATION_SECONDS: f64 = 10.5;

/// Read a WAV file and average all channels into one mono track of f32 samples in [-1, 1].
fn load_mono(path: &Path) -> anyhow::Result<(Vec<f32>, u32)> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    println!(
        "Input: sample_rate={} channels={} format={:?} bits={}",
        spec.sample_rate, spec.channels, spec.sample_format, spec.bits_per_sample
    );

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|s| s as f32 / scale))
                .collect::<Result<_, _>>()?
        }
    };

    let channels = spec.channels as usize;
    let mono = interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();

    Ok((mono, spec.sample_rate))
}

/// Resample mono audio to 16 kHz, which is what speakrs expects.
fn resample(input: &[f32], from_rate: u32) -> anyhow::Result<Vec<f32>> {
    if from_rate == TARGET_SAMPLE_RATE {
        return Ok(input.to_vec());
    }

    let mut resampler = FftFixedIn::<f32>::new(
        from_rate as usize,
        TARGET_SAMPLE_RATE as usize,
        1024,
        2,
        1,
    )?;

    let expected_len =
        (input.len() as u64 * TARGET_SAMPLE_RATE as u64 / from_rate as u64) as usize;
    let mut output = Vec::with_capacity(expected_len + resampler.output_delay());

    let mut pos = 0;
    while input.len() - pos >= resampler.input_frames_next() {
        let end = pos + resampler.input_frames_next();
        let chunk = resampler.process(&[&input[pos..end]], None)?;
        output.extend_from_slice(&chunk[0]);
        pos = end;
    }
    // Leftover samples, then one empty call to flush the resampler's internal delay.
    // Skip the leftover call when the input divided evenly into chunks: rubato
    // treats an empty slice as a disabled channel and errors.
    if pos < input.len() {
        let chunk = resampler.process_partial(Some(&[&input[pos..]]), None)?;
        output.extend_from_slice(&chunk[0]);
    }
    let chunk = resampler.process_partial::<&[f32]>(None, None)?;
    output.extend_from_slice(&chunk[0]);

    // Drop the resampler's startup delay so timestamps line up with the original audio.
    let delay = resampler.output_delay().min(output.len());
    output.drain(..delay);
    output.truncate(expected_len);
    Ok(output)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = match args.get(1) {
        Some(p) => Path::new(p),
        None => anyhow::bail!("usage: diarize <audio.wav>"),
    };

    let (mono, sample_rate) = load_mono(path)?;
    let mut audio = resample(&mono, sample_rate)?;
    println!(
        "Prepared {:.1}s of mono {TARGET_SAMPLE_RATE} Hz audio",
        audio.len() as f64 / TARGET_SAMPLE_RATE as f64
    );

    // speakrs scores audio in 10s windows and returns nothing for shorter clips,
    // so pad with trailing silence. Appending at the end keeps timestamps unchanged.
    let min_samples = (MIN_DURATION_SECONDS * TARGET_SAMPLE_RATE as f64) as usize;
    if audio.len() < min_samples {
        audio.resize(min_samples, 0.0);
    }

    println!("Loading diarization models (first run downloads them)...");
    let mut pipeline = OwnedDiarizationPipeline::from_pretrained(ExecutionMode::CoreMl)
        .map_err(|e| anyhow::anyhow!("failed to load pipeline: {e}"))?;
    let result = pipeline
        .run(&audio)
        .map_err(|e| anyhow::anyhow!("diarization failed: {e}"))?;

    println!("\n== Speaker turns ==");
    for segment in &result.segments {
        println!(
            "  {:>8.2}s - {:>8.2}s  {}",
            segment.start, segment.end, segment.speaker
        );
    }

    let mut talk_time: BTreeMap<&str, f64> = BTreeMap::new();
    for segment in &result.segments {
        *talk_time.entry(&segment.speaker).or_default() += segment.duration();
    }

    println!("\n== Speakers: {} ==", talk_time.len());
    for (speaker, seconds) in &talk_time {
        println!("  {speaker}: {seconds:.1}s");
    }

    Ok(())
}

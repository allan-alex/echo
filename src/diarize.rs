use std::collections::BTreeMap;
use std::path::Path;

use nnnoiseless::DenoiseState;
use rubato::{FftFixedIn, Resampler};
use speakrs::{ExecutionMode, OwnedDiarizationPipeline};

const TARGET_SAMPLE_RATE: u32 = 16_000;
const DENOISE_SAMPLE_RATE: u32 = 48_000;
const MIN_DURATION_SECONDS: f64 = 10.5;
const TURN_GAP_SECONDS: f64 = 0.5;

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

/// Resample mono audio between sample rates.
fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> anyhow::Result<Vec<f32>> {
    if from_rate == to_rate {
        return Ok(input.to_vec());
    }

    let mut resampler =
        FftFixedIn::<f32>::new(from_rate as usize, to_rate as usize, 1024, 2, 1)?;

    let expected_len = (input.len() as u64 * to_rate as u64 / from_rate as u64) as usize;
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

/// Remove background noise with RNNoise. Input must be 48 kHz mono in [-1, 1].
fn denoise(input: &[f32]) -> Vec<f32> {
    const FRAME: usize = DenoiseState::FRAME_SIZE;
    // RNNoise works on 16-bit-range samples and delays its output by one frame.
    const SCALE: f32 = i16::MAX as f32;

    let mut state = DenoiseState::new();
    let mut output = Vec::with_capacity(input.len() + FRAME);
    let mut in_frame = [0.0f32; FRAME];
    let mut out_frame = [0.0f32; FRAME];

    // One extra frame of trailing silence flushes the delayed final frame.
    for start in (0..input.len() + FRAME).step_by(FRAME) {
        in_frame.fill(0.0);
        let end = (start + FRAME).min(input.len());
        if start < end {
            for (dst, &src) in in_frame.iter_mut().zip(&input[start..end]) {
                *dst = src * SCALE;
            }
        }
        state.process_frame(&mut out_frame, &in_frame);
        output.extend(out_frame.iter().map(|&s| s / SCALE));
    }

    // Dropping the first frame removes both the delay and RNNoise's fade-in artifacts.
    output.drain(..FRAME);
    output.truncate(input.len());
    output
}

fn write_mono_wav(path: &Path, samples: &[f32], sample_rate: u32) -> anyhow::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &sample in samples {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = match args.get(1) {
        Some(p) => Path::new(p),
        None => anyhow::bail!("usage: diarize <audio.wav> [--no-denoise]"),
    };
    let use_denoise = !args.iter().any(|a| a == "--no-denoise");

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let out_dir = path.with_file_name(format!("{stem}_speakers"));
    std::fs::create_dir_all(&out_dir)?;

    let (mut mono, mut sample_rate) = load_mono(path)?;
    if use_denoise {
        mono = denoise(&resample(&mono, sample_rate, DENOISE_SAMPLE_RATE)?);
        sample_rate = DENOISE_SAMPLE_RATE;
        let file = out_dir.join("denoised.wav");
        write_mono_wav(&file, &mono, sample_rate)?;
        println!("Denoised audio: {}", file.display());
    }

    let mut audio = resample(&mono, sample_rate, TARGET_SAMPLE_RATE)?;
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

    export_speakers(&mono, sample_rate, &result.segments, &out_dir)?;

    Ok(())
}

/// Write one WAV per speaker containing only their turns, cut from the original
/// full-quality audio and joined with a short silence so turns stay distinguishable.
fn export_speakers(
    mono: &[f32],
    sample_rate: u32,
    segments: &[speakrs::Segment],
    out_dir: &Path,
) -> anyhow::Result<()> {
    if segments.is_empty() {
        return Ok(());
    }

    let mut clips: BTreeMap<&str, Vec<f32>> = BTreeMap::new();
    let gap = vec![0.0; (TURN_GAP_SECONDS * sample_rate as f64) as usize];
    for segment in segments {
        let start = ((segment.start * sample_rate as f64) as usize).min(mono.len());
        let end = ((segment.end * sample_rate as f64) as usize).min(mono.len());
        let clip = clips.entry(&segment.speaker).or_default();
        if !clip.is_empty() {
            clip.extend_from_slice(&gap);
        }
        clip.extend_from_slice(&mono[start..end]);
    }

    println!("\n== Speaker audio ==");
    for (speaker, samples) in &clips {
        let file = out_dir.join(format!("{speaker}.wav"));
        write_mono_wav(&file, samples, sample_rate)?;
        println!("  {}", file.display());
    }

    Ok(())
}

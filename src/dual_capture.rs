use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use hound::{WavSpec, WavWriter};

type SharedWriter = Arc<Mutex<Option<WavWriter<std::io::BufWriter<std::fs::File>>>>>;

fn find_input_device(host: &cpal::Host, needle: &str) -> Option<Device> {
    host.input_devices().ok()?.find(|d| {
        d.name()
            .map(|n| n.to_lowercase().contains(&needle.to_lowercase()))
            .unwrap_or(false)
    })
}

fn build_capture_stream(
    device: &Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    writer: SharedWriter,
    label: &'static str,
) -> anyhow::Result<Stream> {
    let err_fn = move |err| eprintln!("[{label}] stream error: {err}");

    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(mut guard) = writer.lock() {
                    if let Some(w) = guard.as_mut() {
                        for &sample in data {
                            let _ = w.write_sample(sample);
                        }
                    }
                }
            },
            err_fn,
            None,
        )?,
        SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                if let Ok(mut guard) = writer.lock() {
                    if let Some(w) = guard.as_mut() {
                        for &sample in data {
                            let _ = w.write_sample(sample as f32 / i16::MAX as f32);
                        }
                    }
                }
            },
            err_fn,
            None,
        )?,
        other => anyhow::bail!("[{label}] unsupported sample format: {other:?}"),
    };

    Ok(stream)
}

fn start_capture(
    host: &cpal::Host,
    device: Device,
    output_path: &str,
    label: &'static str,
) -> anyhow::Result<(Stream, SharedWriter)> {
    let config = device.default_input_config()?;
    println!(
        "[{label}] using '{}' — sample_rate={} channels={} format={:?}",
        device.name()?,
        config.sample_rate().0,
        config.channels(),
        config.sample_format()
    );

    let spec = WavSpec {
        channels: config.channels(),
        sample_rate: config.sample_rate().0,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let writer = WavWriter::create(output_path, spec)?;
    let writer: SharedWriter = Arc::new(Mutex::new(Some(writer)));

    let stream = build_capture_stream(
        &device,
        &config.clone().into(),
        config.sample_format(),
        writer.clone(),
        label,
    )?;

    stream.play()?;
    let _ = host;
    Ok((stream, writer))
}

fn main() -> anyhow::Result<()> {
    let host = cpal::default_host();

    let mic_device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("no default microphone found"))?;
    let (mic_stream, mic_writer) = start_capture(&host, mic_device, "mic.wav", "MIC")?;

    let blackhole_device = find_input_device(&host, "blackhole").ok_or_else(|| {
        anyhow::anyhow!(
            "BlackHole device not found. Install it with `brew install blackhole-2ch` \
             and route your Sound Output through a Multi-Output Device that includes it."
        )
    })?;
    let (system_stream, system_writer) =
        start_capture(&host, blackhole_device, "system_audio.wav", "SYSTEM")?;

    println!("Recording BOTH mic.wav and system_audio.wav — press Ctrl+C to stop.");

    let running = Arc::new(Mutex::new(true));
    let running_clone = running.clone();
    ctrlc::set_handler(move || {
        *running_clone.lock().unwrap() = false;
    })?;

    while *running.lock().unwrap() {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    drop(mic_stream);
    drop(system_stream);

    if let Some(w) = mic_writer.lock().unwrap().take() {
        w.finalize()?;
    }
    if let Some(w) = system_writer.lock().unwrap().take() {
        w.finalize()?;
    }

    println!("Saved mic.wav and system_audio.wav");
    Ok(())
}
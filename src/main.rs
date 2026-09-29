use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{WavSpec, WavWriter};

fn find_device_by_name(host: &cpal::Host, target: &str) -> Option<cpal::Device> {
    host.input_devices().ok()?.find(|d| {
        d.name().map(|n| n.contains(target)).unwrap_or(false)
    })
}

fn main() -> anyhow::Result<()> {
    let host = cpal::default_host();
    let device = find_device_by_name(&host, "BlackHole")
        .ok_or_else(|| anyhow::anyhow!("BlackHole device not found — is it installed?"))?;

    println!("Using: {}", device.name()?);

    let config = device.default_input_config()?;
    println!(
        "Config: sample_rate={} channels={} format={:?}",
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

    let output_path = "capture.wav";
    let writer = WavWriter::create(output_path, spec)?;
    let writer = Arc::new(Mutex::new(Some(writer)));

    let writer_clone = writer.clone();
    let err_fn = |err| eprintln!("stream error: {err}");

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(mut guard) = writer_clone.lock() {
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
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                if let Ok(mut guard) = writer_clone.lock() {
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
        other => anyhow::bail!("unsupported sample format: {other:?}"),
    };

    stream.play()?;
    println!("Recording... press Ctrl+C to stop.");

    let running = Arc::new(Mutex::new(true));
    let running_clone = running.clone();
    ctrlc::set_handler(move || {
        *running_clone.lock().unwrap() = false;
    })?;

    while *running.lock().unwrap() {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    drop(stream);

    if let Some(w) = writer.lock().unwrap().take() {
        w.finalize()?;
    }

    println!("Saved to {output_path}");
    Ok(())
}
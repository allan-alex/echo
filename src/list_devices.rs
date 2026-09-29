use cpal::traits::{DeviceTrait, HostTrait};

fn main() -> anyhow::Result<()> {
    let host = cpal::default_host();

    println!("== Input devices ==");
    for (i, device) in host.input_devices()?.enumerate() {
        let name = device.name().unwrap_or_else(|_| "<unknown>".to_string());
        let default_marker = match host.default_input_device() {
            Some(d) if d.name().ok() == Some(name.clone()) => " (default)",
            _ => "",
        };
        println!("  [{i}] {name}{default_marker}");

        if let Ok(config) = device.default_input_config() {
            println!(
                "        sample_rate={} channels={} format={:?}",
                config.sample_rate().0,
                config.channels(),
                config.sample_format()
            );
        }
    }

    println!("\n== Output devices ==");
    for (i, device) in host.output_devices()?.enumerate() {
        let name = device.name().unwrap_or_else(|_| "<unknown>".to_string());
        println!("  [{i}] {name}");
    }

    Ok(())
}
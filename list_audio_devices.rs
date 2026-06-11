use cpal::traits::{DeviceTrait, HostTrait};

fn main() {
    let host = cpal::default_host();
    println!("Default host: {:?}", host.id());
    if let Some(device) = host.default_output_device() {
        println!("Default output device: {}", device.name().unwrap_or_default());
    } else {
        println!("No default output device");
    }

    println!("All devices:");
    if let Ok(devices) = host.output_devices() {
        for (i, d) in devices.enumerate() {
            println!("  {}: {}", i, d.name().unwrap_or_default());
        }
    }
}

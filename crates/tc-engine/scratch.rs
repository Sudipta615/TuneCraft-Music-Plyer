use cpal::traits::{DeviceTrait, HostTrait};

fn main() {
    let host = cpal::default_host();
    if let Some(device) = host.default_output_device() {
        println!("Default device name: {}", device.name().unwrap_or_default());
    } else {
        println!("No default device");
    }
}

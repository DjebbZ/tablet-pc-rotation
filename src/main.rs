//! The goal of this program is to rotate the display of my Linux laptop every time I rotate it,
//! like going from normal mode to tent mode, or landscape <=> portrait.
//!
//! Linux sysfs exposes the various accelerometer analog values via the Linux IIO subsystem.
//! The iio exposes values produced by the analog device and performs a Analog to Digital Conversion
//! (ADC). So it's a poll model, I think there's no way of getting notified when the values change.  
//! I've found some interesting documentation about the IIO subsystem here:
//! https://wiki.analog.com/software/linux/docs/iio/iio
//!
//! In my laptop (Lenovo Yoga C940) there's only one accelerometer that captures
//! the screen orientation in space in the 3 axes (x y z). The code below details the meaning of
//! these values.
//!
//! This program is an adaptation of a python script referenced in the Arch Linux wiki:
//! https://gist.githubusercontent.com/ei-grad/4d9d23b1463a99d24a8d/raw/rotate.py
//! It's also an attempt to learn Rust by doing something useful for me.

use std::fs::read_to_string;
use std::io::Error;
use std::thread::sleep;
use std::time::Duration;

fn read_value(path: &str) -> f64 {
    let raw = read_to_string(path)
        .expect(format!("Couldn't open file {}\nDo you have an accelerometer?", path).as_str());

    if let Ok(value) = raw.trim().parse::<f64>() {
        value
    } else {
        // Maybe it's a integer, try again
        if let Ok(value) = raw.trim().parse::<isize>() {
            value as f64
        } else {
            println!("Can't parse content of {}", path);
            panic!();
        }
    }
}

fn main() -> Result<(), Error> {
    loop {
        let accel_x_raw = read_value("/sys/bus/iio/devices/iio:device0/in_accel_x_raw");
        let accel_y_raw = read_value("/sys/bus/iio/devices/iio:device0/in_accel_y_raw");
        let accel_z_raw = read_value("/sys/bus/iio/devices/iio:device0/in_accel_z_raw");
        let scale = read_value("/sys/bus/iio/devices/iio:device0/in_accel_scale");
        let offset = read_value("/sys/bus/iio/devices/iio:device0/in_accel_offset");

        // accel_x == 0 : the device is horizontal
        // accel_x < 0 : the device is in portrait mode, top of screen to the left
        // accel_x > 0 : the device is in portrait mode, top of screen to the right
        let accel_x = (accel_x_raw + offset) * scale;

        // accel_y ~== -9.80 : the screen is vertical
        // ~-9.80 <= accel_y <= 0 : the screen is going horizontal
        // 0 >= accel_y >= ~9.80 : the screen is going vertical with its head pointing to the ground
        let accel_y = (accel_y_raw + offset) * scale;

        // 0 keyboard is horizontal
        // 0 < z < ~10 keyboard is facing the user
        // ~-10 < z < 0 keyboard is facing forward
        let accel_z = (accel_z_raw + offset) * scale;

        println!("x: {}", accel_x);
        println!("y: {}", accel_y);
        println!("z: {}", accel_z);

        sleep(Duration::from_secs(2));
    }
}

//! The goal of this program is to rotate the display of my Linux convertible laptop every time
//! I rotate it, like going from normal mode to tent mode, or landscape <=> portrait
//! and adjust/disable/re-enable the various input methods (touchscreen, touchpad, keyboard)
//! accordingly.
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
use std::process::Command;
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

/// Using xrandr, rotate the current output to the specified orientation and adjust the inputs
/// accordingly.
fn rotate(orientation: LaptopOrientation) {
    // instantiate once to avoid duplication
    let mut xrandr = Command::new("xrandr");
    let mut _xinput = Command::new("xinput");

    match orientation {
        LaptopOrientation::Normal => {
            xrandr
                .args(&["--orientation", "normal"])
                .status()
                .expect("Failed to rotate in normal orientation");
        }
        LaptopOrientation::PortraitLeft => {
            xrandr
                .args(&["--orientation", "right"])
                .status()
                .expect("Failed to rotate the screen right");
        }
        LaptopOrientation::PortraitRight => {
            xrandr
                .args(&["--orientation", "left"])
                .status()
                .expect("Failed to rotate the screen left");
        }
        LaptopOrientation::Tent => {
            xrandr
                .args(&["--orientation", "inverted"])
                .status()
                .expect("Failed to invert the screen orientation");
        }
        LaptopOrientation::Tablet => {
            xrandr
                .args(&["--orientation", "normal"])
                .status()
                .expect("Failed to rotate in normal orientation");
        }
    }
}

/// Calculate the proper value collected by the iio after the ADC
fn normalize(value: f64, scale: f64, offset: f64) -> f64 {
    (value + offset) * scale
}

enum LaptopOrientation {
    Normal,
    PortraitLeft,
    PortraitRight,
    Tent,
    Tablet,
}

/// The accelerometer seems to be in the screen. Here are the values reported from the iio in sysfs
/// depending on the screen orientation. The values usually range from approx. -10 to approx. 10.
///
/// Legend :
/// - the smaller rectangle is top notch of the screen
/// - inside the rectangle 'o' is the webcam
/// - the bigger rectangle is the screen
///
/// +---------+    Laptop is in "normal" laptop mode, screen facing the user.
/// |    o    |
/// +---------+    x = 0
/// |         |    y = -10
/// |         |    z = 0
/// +---------+
///
/// +---------+    Laptop screen is upside down, screen facing the user.
/// |         |
/// |         |    x = 0
/// +---------+    y ~= 10
/// |    o    |    z = 0
/// +---------+
///
/// +-+-----+      Laptop is rotated left, screen facing the user.
/// | |     |
/// | |     |
/// |o|     |      x ~= -10
/// | |     |      y = 0
/// | |     |      z = 0
/// +-+-----+
///
/// +-----+-+      Screen is rotated right, facing the user.
/// |     | |
/// |     | |
/// |     |o|      x ~= 10
/// |     | |      y = 0
/// |     | |      z = 0
/// +-----+-+
///
/// +---------+    Screen is rotated left, facing the sky.
/// |    o    |
/// +---------+    x = 0
/// |         |    y = 0
/// |         |    z ~= -10
/// +---------+
///
/// +---------+    Screen is horizontal, facing the ground.
/// |    o    |
/// +---------+    x = 0
/// |         |    y = 0
/// |         |    z ~= 10
/// +---------+
///
/// Since there's no accelerometer in the keyboard shell there's no way to know its orientation.
/// We'll deduce the overall laptop orientation based on "common sense" and "usability".
#[derive(Debug, PartialEq)]
struct Accelerometer {
    x: f64,
    y: f64,
    z: f64,
}

impl Accelerometer {
    pub fn new(x: f64, y: f64, z: f64, scale: f64, offset: f64) -> Accelerometer {
        Accelerometer {
            x: dbg!(normalize(x, scale, offset)),
            y: dbg!(normalize(y, scale, offset)),
            z: dbg!(normalize(z, scale, offset)),
        }
    }

    /// The ranges chosen here are arbitrary and based on my own experience with the device.
    /// They're voluntarily a bit large to allow for detecting the next orientation before the user
    /// actually finished rotating the device with some margin of error (nobody will have a laptop
    /// perfectly vertical for instance), so that hopefully when he's done the intended orientation
    /// has already been detected.
    pub fn which_orientation(&self) -> LaptopOrientation {
        if (-11.0..=-5.0).contains(&self.x) {
            LaptopOrientation::PortraitLeft
        } else if (5.0..=11.0).contains(&self.x) {
            LaptopOrientation::PortraitRight
        } else if (-11.0..=-7.0).contains(&self.z) {
            // Here we assume that when the screen is close to horizontal facing the sky,
            // the user did put the keyboard behind the screen in "tablet" mode.
            LaptopOrientation::Tablet
        } else if (7.0..=11.0).contains(&self.y) {
            LaptopOrientation::Tent
        } else {
            // safe fallback
            LaptopOrientation::Normal
        }
    }
}

fn main() {
    loop {
        let accel_x_raw = read_value("/sys/bus/iio/devices/iio:device0/in_accel_x_raw");
        let accel_y_raw = read_value("/sys/bus/iio/devices/iio:device0/in_accel_y_raw");
        let accel_z_raw = read_value("/sys/bus/iio/devices/iio:device0/in_accel_z_raw");
        let scale = read_value("/sys/bus/iio/devices/iio:device0/in_accel_scale");
        let offset = read_value("/sys/bus/iio/devices/iio:device0/in_accel_offset");

        match Accelerometer::new(accel_x_raw, accel_y_raw, accel_z_raw, scale, offset)
            .which_orientation()
        {
            LaptopOrientation::Normal => rotate(LaptopOrientation::Normal),
            LaptopOrientation::PortraitLeft => rotate(LaptopOrientation::PortraitLeft),
            LaptopOrientation::PortraitRight => rotate(LaptopOrientation::PortraitRight),
            LaptopOrientation::Tent => rotate(LaptopOrientation::Tent),
            LaptopOrientation::Tablet => rotate(LaptopOrientation::Tablet),
        }

        sleep(Duration::from_secs(5));
    }
}

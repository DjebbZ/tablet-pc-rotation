//! The goal of this program is to rotate the display of my Linux convertible laptop every time
//! I rotate it, like going from normal mode to tent mode, or landscape <=> portrait
//! and adjust/disable/re-enable the various input methods (touchscreen, touchpad, keyboard)
//! accordingly.
//!
//! Linux sysfs exposes the various accelerometer analog values via the Linux IIO subsystem.
//! The iio exposes values produced by the analog device and performs a Analog to Digital Conversion
//! (ADC). So it's a poll model, I think there's no way of getting notified when the values change.
//! I've found some interesting documentation about the IIO subsystem here:
//! <https://wiki.analog.com/software/linux/docs/iio/iio>
//!
//! In my laptop (Lenovo Yoga C940) there's only one accelerometer that captures
//! the screen orientation in space in the 3 axes (x y z). The code below details the meaning of
//! these values.
//!
//! This program is an adaptation of a python script referenced in the Arch Linux wiki:
//! <https://gist.githubusercontent.com/ei-grad/4d9d23b1463a99d24a8d/raw/rotate.py>
//! It's also an attempt to learn Rust by doing something useful for me.
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
use std::fs::read_to_string;
use std::io;
use std::num::ParseIntError;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

#[derive(Debug)]
enum ReadError {
    IOError(io::Error),
    ParseError(ParseIntError),
}

impl From<io::Error> for ReadError {
    fn from(error: io::Error) -> Self {
        ReadError::IOError(error)
    }
}

impl From<ParseIntError> for ReadError {
    fn from(error: ParseIntError) -> Self {
        ReadError::ParseError(error)
    }
}

fn read_value(path: &str) -> Result<f64, ReadError> {
    let raw = read_to_string(path)?;

    if let Ok(value) = raw.trim().parse::<f64>() {
        Ok(value)
    } else {
        // Maybe it's a integer, try again
        let value = raw.trim().parse::<i32>()?;
        Ok(f64::from(value))
    }
}

#[derive(Debug)]
enum RotationError<'a> {
    ExecError(io::Error),
    RotateScreen(&'a str),
    DisableKeyboard,
    EnableKeyboard,
    DisableTouchpad,
    EnableTouchpad,
}

/// Helper function to reduce duplication of code when invoking an external command
fn invoke<'a>(command: &mut Command, err_msg: &'a str) -> Result<(), RotationError<'a>> {
    match command.status() {
        Ok(status) => {
            if status.success() {
                Ok(())
            } else {
                Err(RotationError::RotateScreen(err_msg))
            }
        }
        Err(err) => Err(RotationError::ExecError(err)),
    }
}

/// Using xrandr, rotate the current output based on the laptop orientation.
fn rotate<'a>(orientation: &LaptopOrientation) -> Result<(), RotationError<'a>> {
    // instantiate once to avoid duplication
    let mut xrandr = Command::new("xrandr");

    match orientation {
        LaptopOrientation::Normal | LaptopOrientation::Tablet => invoke(
            xrandr.args(&["--orientation", "normal"]),
            "xrandr couldn't rotate screen in normal orientation",
        ),
        LaptopOrientation::PortraitLeft => invoke(
            xrandr.args(&["--orientation", "right"]),
            "xrandr couldn't rotate screen right",
        ),
        LaptopOrientation::PortraitRight => invoke(
            xrandr.args(&["--orientation", "left"]),
            "xrandr couldn't rotate screen to the left",
        ),
        LaptopOrientation::Tent => invoke(
            xrandr.args(&["--orientation", "inverted"]),
            "xrandr couldn't rotate screen 180\u{b0}",
        ),
    }
}

/// Using xinput, adjust the inputs according to the laptop orientation.
// fn adjust_inputs(orientation: LaptopOrientation) -> Result<(), RotationError> {
//     let mut xinput = Command::new("xinput");
//
//     match orientation {
//         LaptopOrientation::Normal {}
//     }
// }

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
/// +---------+    Screen is horizontal, facing the sky.
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

#[derive(Debug)]
enum ProgramError<'a> {
    RotationError(RotationError<'a>),
    ReadError(ReadError),
}

impl From<ReadError> for ProgramError<'_> {
    fn from(read_err: ReadError) -> Self {
        ProgramError::ReadError(read_err)
    }
}

impl<'p> From<RotationError<'p>> for ProgramError<'p> {
    fn from(rotate_err: RotationError<'p>) -> Self {
        ProgramError::RotationError(rotate_err)
    }
}

fn main() {
    loop {
        let accel_x = read_value("/sys/bus/iio/devices/iio:device0/in_accel_x_raw").unwrap();
        let accel_y = read_value("/sys/bus/iio/devices/iio:device0/in_accel_y_raw").unwrap();
        let accel_z = read_value("/sys/bus/iio/devices/iio:device0/in_accel_z_raw").unwrap();
        let scale = read_value("/sys/bus/iio/devices/iio:device0/in_accel_scale").unwrap();
        let offset = read_value("/sys/bus/iio/devices/iio:device0/in_accel_offset").unwrap();

        let current_orientation =
            Accelerometer::new(accel_x, accel_y, accel_z, scale, offset).which_orientation();
        rotate(&current_orientation).unwrap();

        sleep(Duration::from_secs(5));
    }
}

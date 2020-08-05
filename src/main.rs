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
use std::io::{self, Error, ErrorKind};
use std::num::ParseIntError;
use std::path::Path;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

// --------------------------------------
//
// Gather the inputs of the program
//
// --------------------------------------

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

/// Read the file and return its content, which is supposed to be a single value in a single line.
fn read_value(path: &Path) -> Result<f64, ReadError> {
    let raw = read_to_string(path)
        .map_err(|_| io::Error::new(ErrorKind::NotFound, format!("file {:?} not found", path)))?;

    // TODO: simplify the control flow with `or_else` chaining. Didn't manage yet.
    if let Ok(value) = raw.trim().parse::<f64>() {
        Ok(value)
    } else {
        // Maybe it's a integer, try again
        let value = raw.trim().parse::<i32>()?;
        Ok(f64::from(value))
    }
}

/// Using xinput, list the available inputs.
fn list_input_devices() -> io::Result<Vec<String>> {
    let output = Command::new("xinput")
        .args(&["list", "--name-only"])
        .output()
        .expect("Failed to run xinput, is it properly installed?");

    if !output.status.success() {
        panic!("xinput failed to list the inputs.");
    }

    let output =
        String::from_utf8(output.stdout).map_err(|err| Error::new(ErrorKind::Other, err))?;

    let inputs: Vec<String> = output
        .lines()
        .map(std::string::ToString::to_string)
        .collect();

    Ok(inputs)
}

// --------------------------------------
//
// Model the problem
//
// --------------------------------------

/// Representation of the various physical modes of using the laptop. The orientation described are
/// those that makes the most sense and assume that unless in normal mode the keyboard is not meant
/// be used and retracted behind the screen.
enum LaptopOrientation {
    /// "normal mode", the laptop is opened, keyboard horizontal and screen vertical
    Normal,
    /// From normal mode, rotate the laptop to the left
    PortraitLeft,
    /// From normal mode, rotate the laptop to the left
    PortraitRight,
    /// Screen upside down facing the user , keyboard vertical behind the screen, with just enough angle
    /// so that the laptop can stand
    Tent,
    /// Screen horizontal with the keyboard behind, like a drawing tablet
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
            x: normalize(x, scale, offset),
            y: normalize(y, scale, offset),
            z: normalize(z, scale, offset),
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

/// Calculate the proper value collected by the iio after the ADC.
fn normalize(value: f64, scale: f64, offset: f64) -> f64 {
    (value + offset) * scale
}

// --------------------------------------
//
// The side-effects
//
// --------------------------------------

/// Helper function to reduce duplication of code when calling xrandr.
fn call_xrandr(orientation: &str, err_msg: &str) -> io::Result<()> {
    let status = Command::new("xrandr")
        .args(&["--orientation", orientation])
        .status()
        .expect("Couldn't run xrandr, is it properly installed?");

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(ErrorKind::Other, err_msg))
    }
}

/// Using xrandr, rotate the current output based on the laptop orientation.
fn rotate_screen_output(orientation: &LaptopOrientation) -> io::Result<()> {
    match orientation {
        LaptopOrientation::Normal | LaptopOrientation::Tablet => call_xrandr(
            "normal",
            "xrandr couldn't rotate screen in normal orientation",
        )?,
        LaptopOrientation::PortraitLeft => {
            call_xrandr("right", "xrandr couldn't rotate screen right")?
        }
        LaptopOrientation::PortraitRight => {
            call_xrandr("left", "xrandr couldn't rotate screen to the left")?
        }
        LaptopOrientation::Tent => {
            call_xrandr("inverted", "xrandr couldn't rotate screen 180\u{b0}")?
        }
    };

    Ok(())
}

/// Helper that returns elements in `inputs` that match the elements in `to_find`.
/// Elements in `to_find` must be substrings of elements in `inputs`.
fn find_inputs<'a>(inputs: &'a [String], to_find: &'a [String]) -> Vec<&'a String> {
    inputs
        .iter()
        .filter(|device| {
            to_find.iter().any(|find_me| {
                device
                    .to_ascii_lowercase()
                    .contains(&find_me.to_ascii_lowercase())
            })
        })
        .collect::<Vec<&String>>()
}

/// Using `xinput`, enable or disable the input devices.
fn toggle_inputs(inputs: &[&String], enable: bool) -> io::Result<()> {
    for input in inputs {
        let action = if enable { "enable" } else { "disable" };
        let failure_msg = format!("xinput couldn't {} {}", action, input);
        let status = Command::new("xinput")
            .arg(action)
            .arg(input) // `keyboard[0]` because I suppose there should be only one integrated keyboard in a laptop
            .status()
            .expect("Couldn't run `xinput`, are you sure it's installed properly?");
        if !status.success() {
            return Err(io::Error::new(ErrorKind::Other, failure_msg));
        }
    }

    Ok(())
}

/// Using `xinput`, enable/disable the laptop keyboard depending on the orientation.
fn toggle_keyboard(orientation: &LaptopOrientation, inputs: &[String]) -> io::Result<()> {
    // Singular tense because there should be only one internal keyboard in a laptop, right?
    let keyboard_to_find = &[String::from("AT Translated Set 2 keyboard")];
    let keyboard: Vec<&String> = find_inputs(inputs, keyboard_to_find);

    if keyboard.is_empty() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "No keyboard found"));
    }

    match orientation {
        LaptopOrientation::Normal => toggle_inputs(&keyboard, true)?,
        LaptopOrientation::PortraitLeft
        | LaptopOrientation::PortraitRight
        | LaptopOrientation::Tent
        | LaptopOrientation::Tablet => toggle_inputs(&keyboard, false)?,
    }

    Ok(())
}

/// Using `xinput`, rotate the screen inputs (touchscreen or stylus). Without this when the screen
/// output is rotated touching part of the screen moves the cursor elsewhere.
fn rotate_screen_inputs(orientation: &LaptopOrientation, inputs: &[String]) -> io::Result<()> {
    let screen_inputs_to_find = &[String::from("touchscreen"), String::from("wacom")];
    let screen_inputs = find_inputs(inputs, screen_inputs_to_find);

    if screen_inputs.is_empty() {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            "no touchscreen or wacom inputs found",
        ));
    }

    let transformation_matrix = match orientation {
        LaptopOrientation::Normal | LaptopOrientation::Tablet => [1, 0, 0, 0, 1, 0, 0, 0, 1],
        LaptopOrientation::PortraitLeft => [0, 1, 0, -1, 0, 1, 0, 0, 1],
        LaptopOrientation::PortraitRight => [0, -1, 1, 1, 0, 0, 0, 0, 1],
        LaptopOrientation::Tent => [-1, 0, 1, 0, -1, 1, 0, 0, 1],
    };

    for input in screen_inputs {
        let mut xinput = Command::new("xinput");
        let command = xinput
            .arg("set-prop")
            .arg(input)
            .arg("Coordinate Transformation Matrix");

        for number in &transformation_matrix {
            command.arg(number.to_string());
        }

        let status = command
            .status()
            .expect("Couldn't run `xinput`, are you sure it's installed properly?");

        if !status.success() {
            return Err(io::Error::new(
                ErrorKind::Other,
                format!("xinput couldn't rotate '{}'", input),
            ));
        }
    }

    Ok(())
}

/// Using `xinput`, enable/disable touchpads (physical integrated inputs that move the mouse cursor).
fn toggle_touchpads(orientation: &LaptopOrientation, inputs: &[String]) -> io::Result<()> {
    let touchpad_to_find = &[String::from("Touchpad"), String::from("Trackpoint")];
    let touchpads = find_inputs(inputs, touchpad_to_find);

    if touchpads.is_empty() {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            "no touchpad or trackpoint found",
        ));
    }

    match orientation {
        LaptopOrientation::Normal => toggle_inputs(&touchpads, true)?,
        LaptopOrientation::PortraitLeft
        | LaptopOrientation::PortraitRight
        | LaptopOrientation::Tent
        | LaptopOrientation::Tablet => toggle_inputs(&touchpads, false)?,
    }

    Ok(())
}

// --------------------------------------
//
// Core app. INPUT -> MODEL -> OUTPUT
//
// --------------------------------------

fn main() {
    loop {
        let accel_x =
            read_value(Path::new("/sys/bus/iio/devices/iio:device0/in_accel_x_raw")).unwrap();
        let accel_y =
            read_value(Path::new("/sys/bus/iio/devices/iio:device0/in_accel_y_raw")).unwrap();
        let accel_z =
            read_value(Path::new("/sys/bus/iio/devices/iio:device0/in_accel_z_raw")).unwrap();
        let scale =
            read_value(Path::new("/sys/bus/iio/devices/iio:device0/in_accel_scale")).unwrap();
        let offset = read_value(Path::new(
            "/sys/bus/iio/devices/iio:device0/in_accel_offset",
        ))
        .unwrap();

        let inputs = list_input_devices().unwrap();

        let current_orientation =
            Accelerometer::new(accel_x, accel_y, accel_z, scale, offset).which_orientation();

        rotate_screen_output(&current_orientation)
            .and_then(|_| toggle_keyboard(&current_orientation, &inputs))
            .and_then(|_| toggle_touchpads(&current_orientation, &inputs))
            .and_then(|_| rotate_screen_inputs(&current_orientation, &inputs))
            .unwrap();

        sleep(Duration::from_secs(2));
    }
}

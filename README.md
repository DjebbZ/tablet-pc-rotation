## Tablet pc rotation

The goal of this program is to rotate the display of my Linux convertible laptop every time
I rotate it, like going from normal mode to tent mode, or landscape <=> portrait
and adjust/disable/re-enable the various inputs/output (touchscreen, touchpad, keyboard)
accordingly.

Linux sysfs exposes the various accelerometer analog values via the Linux IIO subsystem.
The iio exposes values produced by the analog device and performs a Analog to Digital Conversion
(ADC). So it's a poll model, I think there's no way of getting notified when the values change.
I've found some interesting documentation about the IIO subsystem here:
<https://wiki.analog.com/software/linux/docs/iio/iio>

In my laptop (Lenovo Yoga C940) there's only one accelerometer that captures
the screen orientation in space in the 3 axes (x y z). The code details the meaning of
these values.

This program is an adaptation of the python script `rotate.py` referenced in the [Arch Linux wiki](https://wiki.archlinux.org/index.php/Tablet_PC#With_xrandr_+_xinput):
<https://gist.githubusercontent.com/ei-grad/4d9d23b1463a99d24a8d/raw/rotate.py>
It's also an attempt to learn Rust by doing something useful for me.

## Usage

- You need a working Rust installation, `X.org`, `xinput` and `xrandr`. Check your distribution on how to install them.
- Clone this repository
- `cargo run --release`
- Rotate your 2-in-1 laptop and see what happens!


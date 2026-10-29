# Widgets

Layouts use fixed Framework 9×34 slot geometry. All pixels, including draft
previews, are generated in Rust. Repeated widgets share source readings but keep
their own presentation. Empty/missing sources do not cause a layout to reflow.

## Clock

The first version supports 24-hour local time (HH above MM), a per-instance
leading-hour-zero toggle, and relative intensity (blank means inherit the clock
default). Minutes are always zero-padded. The renderer takes an injected time;
the runtime samples local wall time every second, so time/date/timezone changes
are not interpreted as monotonic animation time. All instances consume the same
reading rather than running independent clock timers.

A 12-hour option is intentionally not exposed: a legible AM/PM treatment still
needs agreement. Do not silently show 12-hour digits without that distinction.
Physical LED readability is separate from screen previews and requires hardware
qualification; a greyscale screen is not a photometric LED emulator.

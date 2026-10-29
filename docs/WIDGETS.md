# Widgets

## Physical assignment

In the configuration draft, refresh assignment candidates after identifying a
panel visually. Bind each logical panel to its unique physical location, then
Apply. Duplicate bindings are rejected; absent and ambiguous identities never
fall back to USB serial numbers or tty/COM enumeration order. Saved bindings
survive restart/disconnection. Unknown Windows locations remain unassignable
until the native location adapter is qualified on real hardware.

The renderer produces every logical panel in the subscription, including the
second panel preview. A background router enumerates devices every two seconds,
matches only unique stable bindings, holds ports open, coalesces frames per panel
and keeps static displays awake. Unassigned/disconnected modules are not silently
given another panel's role. Unassigning sends sleep and releases the port.

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

## Battery

A single supervised power sampler feeds every battery widget. Linux reads the
first sorted sysfs Battery device (not a fabricated multi-battery aggregate),
capacity/status and AC online state. Windows uses GetSystemPowerStatus; unknown
255 percentages and no-battery flags stay unavailable. Unsupported/malformed
readings are never shown as 0%. The main panel subscription includes the source
status, observation timestamp, charge percentage and independently known AC and
charging flags. Errors blank widgets without stopping the clock.

Sampling is every five seconds, isolated from rendering. A wedged system call
marks the source error after two seconds and waits on the same job, avoiding
unbounded blocking threads while leaving clock updates running. Recovery resumes
sampling. Battery gauges support direction, relative intensity and an optional
low threshold (emphasis at/below it); thresholds for usage sources run in the
opposite direction. A sample-data draft preview remains explicitly synthetic;
the live-time-only draft preview does not claim to sample other sources.

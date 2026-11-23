# Widgets

## Temporary output previews

Render-only sample/live-time previews never change configuration or output. The
explicit hardware preview leases the daemon's desired output for ten seconds
(protocol maximum 15). Only one owner is allowed. A separate local IPC request
stays connected for the lease; EOF, cancellation, expiry or a crashed desktop
ends ownership. The lease is not persisted. Apply can update saved state while
previewing; restoration uses that latest applied state, not a stale original.
Old generation cleanup cannot cancel a replacement lease.

Identification owns the serial port at higher priority; normal routing skips a
busy port and resumes preview/applied output afterward. Serial writes are never
cut short to change priority. Hardware restoration includes the router interval,
wake settle and in-flight command drain, so expiry is not a zero-latency optical
guarantee. Real cancellation/unplug/priority observations require panels.

## Brightness

On Linux, open **Configuration → Brightness → Display and backlight** to
explicitly pair a connected DRM connector with a sysfs backlight. The app
reads the source; it does not change the display's brightness or verify that
the selected backlight controls it. The connector key includes the GPU device
name instead of the card enumeration index. This identifies a port, not the
individual monitor plugged into it.

Inherited panel brightness maps the source percentage linearly between the
configured minimum and maximum (zero means minimum, not sleep). Explicit sleep
and per-panel overrides keep precedence. If the connector disappears, the
reading is invalid, or sampling times out, output returns to the saved manual
level; recovery resumes following the same pair. Polling is nominally every
two seconds, in addition to rendering and routing delays. Other curves, Windows
sources and lock/display-off/suspend policies are not implemented here.

Application defaults and explicit per-panel overrides define ordered 1–255
minimum/manual/maximum levels. Manual output is sent through the hardware's
global-brightness command, separately from widget greyscale. Explicit off/sleep
is a separate state, never a numeric level clamped back above zero. Unchecking an
override restores inheritance; overrides remain independent of global defaults.
Source thresholds change relative intensity, not the panel's global ceiling.

Use representative sparse clock/gauge previews while adjusting the actual
hardware. The default minimum of 3 is inherited from POC observations, not a
universal visibility guarantee. Screen previews show greyscale content and off
state; their luminance is not a physical calibration model. Identification is
bounded within the effective min/max and restores the applied brightness/off
policy afterward. Firmware sleep/wake behavior requires panel qualification.

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

Open/access failures retry no sooner than five seconds. Each panel has at most
one in-flight blocking job; later desired frames are coalesced, not queued. A
command-write failure, including a wake or query during startup, quarantines
that connection because a partial serial
payload must not be followed by another command. Reconnect the module and leave
it absent for at least one two-second discovery interval before retrying; rapid
suspend/re-enumeration recovery still requires hardware qualification. Linux
also checks readable process file descriptors for a competing owner; restricted
proc visibility can limit this check, so stop the POC explicitly. Windows uses
exclusive COM access. Serial errors on one panel do not stop the other.

After open/wake settling, firmware identification allows up to three version
queries, each with a two-second reply window and a 300 ms gap between unanswered
queries. The serial read timeout remains 250 ms; timeouts within a reply window
do not discard bytes already received. An incomplete or invalid reply ends the
exchange rather than starting another query. A failed write is never followed
by another query in that exchange. The visible identification outline still
lasts at most two seconds; opening and firmware queries add preparation time.
This bounded retry addresses a missing first query after wake seen in the POC;
physical cold-start and recovery qualification remain required.

Ctrl-C/SIGTERM cancel identification, stop new routing jobs, wait for bounded
in-flight work, and send sleep where possible. Runtime shutdown has a timeout for
wedged blocking OS calls. A hard crash cannot guarantee hardware sleep. Device
event coverage and all real suspend/write-failure behaviors remain hardware
qualification work; the conservative quarantine is not a claim of automatic
parser resynchronization.

Serial-command write failures log the operation stage, opcode, optional column,
host-accepted byte count, elapsed time, error kind and optional OS error. Terminal
firmware-read failures log attempt, received bytes and reply-window elapsed time.
These failure-only records omit payloads and device paths; successful writes do
not confirm firmware parsing. The software-only reproduction and remaining
hardware evidence gaps are in [SERIAL-FAILURE-INVESTIGATION.md](SERIAL-FAILURE-INVESTIGATION.md).

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

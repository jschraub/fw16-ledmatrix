# Widgets

In **Number + bars**, set either bar to **Empty** and select **Center single
bar** to draw the remaining bar across the middle three columns. With two
configured bars, each keeps its side even when the other has no reading.

Five-hour and weekly widgets show the selected provider's **percentage used**.
Choose the provider separately for each slot; no session pin or producer is
required. A missing or expired window stays blank. See [account usage
setup and cache limits](INTEGRATIONS.md#account-usage-windows-linux).

## Temporary output previews

The bay editor automatically renders the selected panel's draft on screen using
the daemon's current readings. Display brightness is illustrative, not calibrated.
Render-only previews never change configuration or physical output. The
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

### Temporary feedback (Linux)

In **Configuration → Feedback**, choose Off or 3–15 seconds for each panel's
volume, brightness and AC-change gauges. Each enabled gauge has its own centered
bar width: 1, 3, 5 (default), 7 or 9 LED columns. Width does not change the
bottom-to-top percentage scale. All effects default off. An AC edge
shows battery percentage if available; a mute shows zero. The brightness gauge
shows screen percentage independently of LED brightness. In manual mode it
uses the sole backlight; if several exist it does not guess. In follow mode it
uses the explicitly selected pair, including on panels with manual overrides.
Known automatic dimming is suppressed
when the existing `$XDG_RUNTIME_DIR/matrixd/brightness-auto` marker is fresh
(four seconds, to allow for polling); without such a signal the adapter cannot
distinguish an unmarked automatic change from a user adjustment.

One gauge replaces another on the same panel; it expires from the latest
observed change. Simultaneous samples use volume before brightness before AC.
The charging pulse changes **whole-panel brightness**. Each panel has its own
strength. At level 32, Subtle rises to 40, Default to 64, and Extreme to 128
over four seconds, returning over the next four. Existing configurations use
Default. Custom takes the requested peak-to-trough spread (1–254 brightness
levels) and the duration of one complete rise and fall (4–60 seconds).
Near the ceiling the range shifts down; the configured minimum can limit contrast.
Hardware previews, identification, sleep
and locked layouts take priority. Expiry returns to current ambient output.
Volume and backlight changes wake rendering and changed panel output wakes routing.
With brightness feedback enabled, Linux samples known backlights roughly every
75 ms; connector discovery stays on the slower two-second cadence. The
one-second ambient tick and two-second device discovery remain fallbacks.
Physical presentation still includes in-flight commands and serial write time;
duration is a desired-output timer.

With volume enabled, sink/server notifications wake a bounded `pactl` reread
of actual volume and mute after a 20 ms coalescing window. A 750 ms poll covers
missed events while the subscription restarts. Startup and recovery establish a baseline. Repeated
identical readings (including notification sounds with unchanged level) do not
trigger a gauge. Changes between samples can be coalesced. The other sources
keep working if audio is missing.

### Brightness and session policies

**When locked:** In Workspace → Configuration → Layout, choose **When locked** for a panel to
keep its normal layout (default), show only the clock, show battery as a full
gauge, or render a blank frame. Apply saves the setting independently for
each panel. Brightness and explicit sleep still apply. Hyprland on Linux is
polled roughly every two seconds, plus render/serial routing delay; switching
is not instantaneous. An unreadable sample retains the last known state;
before the first successful sample or on unsupported desktops the normal
layout is used. Unlock restores the latest desired layout. This is a display
preference, not a privacy guarantee; identification can still flash an outline.

On Linux, open **Configuration → Brightness → Display and backlight** to
explicitly pair a connected DRM connector with a sysfs backlight. The app
reads the source; it does not change the display's brightness or verify that
the selected backlight controls it. The connector key includes the GPU device
name instead of the card enumeration index. This identifies a port, not the
individual monitor plugged into it.

Inherited panel brightness maps the source percentage linearly between the
configured minimum and maximum (zero means minimum, not sleep). Explicit sleep
and per-panel overrides keep precedence during brightness-follow. If the connector disappears, the
reading is invalid, or sampling times out, output returns to the saved manual
level; recovery resumes following the same pair. Polling is nominally every
two seconds, in addition to rendering and routing delays. **Dimmer at low
levels** uses a quadratic curve with the same endpoints. Sampling brightness
only recomputes desired output; it does not generate manual-feedback events.

**Sleep panels when this display turns off** is separate from brightness-follow
and lock content. An explicitly off/standby/suspended DRM connector sleeps
both panels, including overridden panels; an unreadable or disconnected
connector is not evidence of display-off. On wake the daemon resolves the
latest configuration, never a pre-sleep copy. An identification request still
has temporary priority. On Linux the supervised `gdbus monitor` watches logind
`PrepareForSleep` separately; a sleep/wake boundary ends preview leases and
invalidates cached serial handles and frames without clearing quarantine.
It does not inhibit suspend or guarantee completion of in-flight OS writes.

**Windows adapter (physical qualification pending):** active internal WMI
brightness instances are keyed by a digest of their instance name. Unsupported
external displays do not fall back to a different monitor. WTS reports current
session lock state. Power notification callbacks distinguish system suspend from
console display state: `Windows-console` is a console-wide on/off policy, not
per-monitor DPMS. Windows reports dimmed separately from off; dimmed is treated
as on. These sources are not a claim of support for every driver or device.

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

Linux power-supply events trigger a fresh sysfs reading after a 25 ms coalescing
window and wake the renderer. A five-second poll covers missed events or an
unavailable `udevadm monitor`. Sampling is isolated from rendering. A wedged system call
marks the source error after two seconds and waits on the same job, avoiding
unbounded blocking threads while leaving clock updates running. Recovery resumes
sampling. Battery gauges support direction, relative intensity and an optional
low threshold (emphasis at/below it); thresholds for usage sources run in the
opposite direction. A sample-data draft preview remains explicitly synthetic;
the live preview uses the latest machine and assistant readings, leaving
unavailable widgets blank.

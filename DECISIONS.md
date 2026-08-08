# matrix — design decisions

A background daemon driving the two Framework Laptop 16 LED Matrix modules as
ambient status displays.

Everything below marked **[measured]** was verified on this machine on
2026-08-07. Everything marked **[open]** is undecided.

## Hardware facts [measured]

| | |
|---|---|
| Panels | 2 × 9 wide × 34 tall, 306 LEDs, per-LED greyscale |
| USB | `32ac:0020`, CDC-ACM, 115200 8N1 |
| Protocol | `0x32 0xAC` + command byte + payload |
| Firmware | 0.20 on both |
| Commands used | `0x00` brightness (1 byte), `0x03` sleep (1 byte / query), `0x06` DrawBW (39 bytes), `0x20` version (→3 bytes BCD) |

**Both modules report the same USB serial** (`FRAKDEBZ0100000000`), so
`/dev/serial/by-id/` collapses to one symlink and cannot distinguish them.
Address by USB topology instead:

| USB port | tty | bay |
|---|---|---|
| `3.3` | ttyACM0 | **RIGHT** |
| `4.2` | ttyACM1 | **LEFT** |

Note the inversion — the lower-numbered port is the right-hand bay. This maps
*bays*, not modules; the modules are interchangeable.

### Firmware idle sleep — the big constraint

The firmware sleeps on an idle timer (**default 60s, reset by any command**).
A sleeping module does not answer: the first command wakes it and is consumed
doing so, and waking fades the LEDs in over a period during which commands are
not serviced. Consequences:

- An always-on ambient display **must** send traffic more often than the idle
  timer or the firmware blanks it. Keepalive at ~30s.
- Ports are held open for the daemon's lifetime. Reopening costs a 0.2s settle
  plus a wake fade — more than the entire takeover latency budget.
- `Sleep 1` is the correct way to go dark: it powers down the LED controller
  and gives a fade for free, unlike drawing a zeroed bitmap.

## Access

`/etc/udev/rules.d/60-framework-ledmatrix.rules` grants the active-seat user
access via `TAG+="uaccess"` — no `uucp` group membership, which would hand out
every serial device on the machine to reach these two. **[measured]** ACL
verified as `user:jars:rw-`.

Because `uaccess` ACLs are tied to an active seat session, the daemon **must**
be user-scope and session-bound. A system unit would have no ACL.

## Panel assignment

- **LEFT = machine.** Time (rows 1–11, two stacked 2-digit rows) and battery
  (rows 13–33, 3 columns wide, ≈4.8% granularity).
- **RIGHT = Claude.** Context% as a **number** (rows 1–5), then the 5h and 7d
  limits as two bars **side by side** (rows 7–33, columns 1–2 and 6–7).

Zones are **fixed**, never reflowed. The context number goes dark when no
session is running rather than the bars resizing — spatial constancy is the
whole value of an ambient surface, and a layout that moves converts a glance
into a lookup.

**One dark row of padding** separates each digit block from what is below it.
That replaced explicit separator rules, which are gone from both panels: a
blank row and a dim line do the same job, and the blank row does it at every
brightness (see *Rules retired*, below).

### Context is a number, the limits are bars

Context is the one value on either panel worth reading *exactly* rather than
gauging — the difference between 78% and 91% is the difference between carrying
on and wrapping up, and two rows of a bar do not say that. It also gets the top
of the panel, being the value that moves fastest and gets consulted most.

At ≥100 it renders `XX` rather than a number. Three digits need 11 of the 9
available columns, so something has to give; `XX` gives up the value while a
clamped `99` (the earlier behaviour) gives up the *honesty*, since 99 is a
plausible reading and so still looks like data. The clock cannot reach that
branch. Values are **truncated, not rounded**, so `XX` means genuinely at or
past 100.

The two limit bars are 2 columns each with a 3-column gap and a 1-column margin
either side — the only arrangement of those that fits 9. **Side by side rather
than stacked so both get all 27 rows.** That retires an earlier decision to give
7d fewer rows on the grounds that it "rarely nears its ceiling", which had it
backwards: at 6 rows anything under 16.7% was a single dim partial row, so the
band was coarsest exactly across the range it actually occupies. One row is now
3.7%.

The battery bar is likewise **3 columns rather than the full width**. It is a
magnitude, not a picture; narrower leaves the panel reading as a clock with a
gauge beside it rather than a clock sitting on a slab. Its 21 rows are
unchanged, so no resolution is lost.

Orientation is **upright**: the 34px axis runs front-to-back, so text gets only
9px of width — two 3×5 digits, no more. `100%` has no honest text rendering.
This is accepted rather than worked around: the surface shows magnitudes and
two-digit numbers, not language.

All bars are **linear**. A nonlinear scale weighted to the top end was
considered and rejected — it destroys rate reading (steady burn should move the
bar at a steady pace) and reintroduces a memorised scale into the one surface
that should need no parsing. Top-end emphasis goes in brightness instead.

Volume and brightness get **no ambient zone at all** — they are takeover-only.
They are self-knowledge (you set them, you know them), and their value is
concentrated entirely at the instant of change, which the takeover already
covers.

## Interaction model

Ambient by default; a **takeover** fills one panel for ~2s then returns.

Takeovers are for **confirmations only** — volume, brightness, AC plug. Alerts
(battery low, 5h at 90%) instead express themselves in place. An unbidden
takeover arrives with no action available to you and destroys the ability to
read a takeover as "here is the thing you just changed". The right panel never
takes over.

**A context-full pulse was designed and then rejected.** Breathing the right
panel above 80% context works mechanically — `pulse_brightness` already composes
with screen brightness and is floor-safe — but it would have put a *third*
independent variable on the panel's brightness channel, alongside severity on
the bars and working/idle on the number. Three signals sharing one perceptual
channel means a panel at maximum with nothing to say which of the three caused
it. Whole-panel breathing keeps exactly one meaning per panel: on the left, it
means charging. The number itself, and `XX` at the top of its range, are enough
for context.

## Data sources

| Signal | Source | Latency |
|---|---|---|
| 5h / 7d limits | `GET https://api.anthropic.com/api/oauth/usage`, OAuth bearer from `~/.claude/.credentials.json` **[measured]** | 60s poll |
| Context % | statusline shim writing a per-session snapshot; no endpoint exists **[measured]** | on render |
| Volume | `pactl subscribe`, filtered to sink/server events, then re-read **[measured]** | event |
| Brightness | udev `backlight`, plus a marker file to spot automatic changes **[revised]** | event |
| Battery / AC | udev `power_supply` + lazy timer | event |
| Clock | timer | 1 min |

The usage endpoint is **undocumented** — treat a parse failure as "hide the
zone", never as a crash. It also returns a `severity` field per limit, and a
`weekly_scoped` per-model bucket (unused).

Credentials are read **read-only**. The daemon never refreshes the token: that
would rotate the refresh token and rewrite a file Claude Code owns, and a
concurrent refresh would log you out of Claude Code. On 401 the Claude zone
goes stale.

Brightness was going to use a keybind hook rather than udev, **because udev
cannot tell your thumb from a timer** — `hypridle` writes brightness on idle,
and a udev-driven takeover would fire a full-panel gauge at the exact moment you
walked away from the machine.

**That was revised during implementation**, and the reasoning is worth keeping
because the original was answering the wrong question. A keybind hook only fires
for the *keyboard*: brightness changed by a settings slider, a script, or a
docking profile would leave the panels at a stale level indefinitely. That is a
correctness bug, where the thing it was avoiding is a papercut.

So the two questions are separated. **Tracking** the level is udev's job, and
udev sees every change whatever caused it. **Reacting** with a takeover needs
one extra bit that udev genuinely cannot supply, so `idle-dim.sh` supplies it:
it touches a marker file immediately before and after each of its own writes,
and the daemon suppresses the takeover — but not the tracking — when that marker
is fresh.

Marked on both sides of the write on purpose. The kernel emits the uevent during
the write while the daemon reads it asynchronously, so a marker set only before,
or only after, leaves a race in which an automatic change looks deliberate.
Absence of the marker means "deliberate", which is the right way to fail: with
the idle script not installed, every brightness change really is yours.

The Claude session feed's producing half **ships in this repo**
(`integration/claude/`), installed into `~/.claude` by `install.sh`. It lived in
the author's dotfiles first, which was a mistake worth recording: it made the
public repo's Claude panel silently inert for everyone else, and put the
snapshot logic in two places at once.

The status line tap **wraps an existing status line rather than replacing it**,
because Claude Code allows only one `statusLine` command and everyone's is
different. It reads the payload, snapshots it, and passes it through on stdin
untouched — verified byte-identical.

`install.sh` deliberately **does not edit `settings.json`**, only prints what to
add. It is the user's file, it may contain anything, and a bad merge would cost
far more than the paste it saved.

Producer and consumer meet at a directory layout rather than at code, so either
half works with the other absent — no install-order dependency, and no error
noise in someone's Claude Code session when the daemon is not installed.

Values and liveness come from **different sources on purpose**: the status line
renders on a timer, so it can report the context percentage but cannot say
whether a turn is in flight. That edge comes from hooks. Files live under
`XDG_RUNTIME_DIR` because it is tmpfs and clears on logout, so a session killed
without firing `SessionEnd` cannot strand a frozen percentage on the panel.

Volume events are **not trusted, only re-read from**. A PulseAudio sink event
does not mean the volume moved: measured, playing a half-second beep emits two
`change on sink` events with the volume untouched, so an event-as-truth design
would pop a full-panel gauge on every notification sound. The event triggers a
re-read and a takeover fires only if the value actually differs — the same
event-says-look, state-says-what rule as device events. The `pactl` re-read is
safe to run from inside the handler because it emits `client` events, never
`sink` ones, so it cannot retrigger itself.

## Brightness

Panel brightness tracks **screen** brightness, not keyboard backlight. The
keyboard route was investigated and rejected: `rgb:kbd_backlight` does not
exist on this machine (it is EC-controlled, needing `framework-system`, a
privileged `/dev/cros_ec`, and polling since Fn+Space never reaches the OS) —
and more importantly the mapping is backwards, since people raise keyboard
backlight in the dark and turn it off in daylight.

### Visibility is a product, not a level

**[measured]** An LED reads as lit when `global_brightness × greyscale ≳ 520`.
Neither number matters alone. Verified across a 3× range of both:

| element | greyscale | visible at | product |
|---|---|---|---|
| digits | 200 | global 3, not 2 | 600 ✓ / 400 ✗ |
| rules | 60 | global 9, not 8 | 540 ✓ / 480 ✗ |

Calibration:

| screen | panel | note |
|---|---|---|
| 2/100 | 3/255 | floor — lowest at which real content is legible |
| 100/100 | 255/255 | ceiling is the hardware maximum |

> **An earlier figure of 1/255 was wrong and is corrected here.** It came from
> ramping a *solid fill*, which lights all 306 LEDs, and was then applied to
> sparse content that lights maybe a fifth of them. At threshold current a full
> panel reads as a glow while a few thin bands do not register at all. Calibrate
> against representative content, never a fill.

**[open]** whether the curve should match the `-e4` exponential of the existing
brightness keybinds.

### Rules retired — everything is now legible at every brightness

Both panels used to carry separator rules at greyscale 60. Those vanished below
global 9 (~5% screen), and that was defended as correct: at the floor a rule
would need greyscale ≈ 173 to clear the threshold, so close to `DATA` that it
would compete with the data it separated. **You cannot have a rule that is both
visible and subordinate at the floor** — hardware, not tuning — so decoration
was what got dropped.

The rules are gone entirely now, replaced by dark padding rows. A blank row
separates zones just as well, costs the same one row, and does it identically at
every brightness. `draw_rule` and the `RULE` constant are deleted.

The property inverts as a result: **nothing on either panel is allowed to
vanish.** `DATA` and `EMPHASIS` both clear the threshold at `AMBIENT_FLOOR`, so
the panels show the same content at every screen brightness, only dimmer. The
single exception is a bar's partial tip, which is sub-row precision rather than
a value. Pinned by `test_every_meaningful_intensity_survives_the_floor` and, on
the mapping side, `test_the_mapping_keeps_every_element_visible`.

### `DATA` is 180, and it sits on a measurement

`DATA` moved from 200 to 180 to widen the swing up to `EMPHASIS`. The swing is
what you read, and the eye judges it as a **ratio**: 200 → 255 is 1.28×,
180 → 255 is 1.42×. That matters more than it used to, because the context
number's brightness is now a signal in its own right rather than decoration.

It cannot go lower. Visibility is a product, so at `AMBIENT_FLOOR` (global 3)
greyscale 180 lands on 540 — the dimmest product actually *measured* legible.
174 is the arithmetic minimum against the 520 threshold (522 ✓), but 520 is
interpolated between the two measurements in the table above and 540 is one of
them. Sitting on the evidence rather than just past a guess costs 0.05× of
swing. Pinned by `test_data_sits_on_a_measured_product`.

### Takeovers will read brighter than ambient

A full-height gauge is close to a solid fill; an ambient frame is mostly dark.
At equal global brightness the takeover therefore emits far more light. That is
desirable for something meant to grab attention, but it is a consequence of
coverage rather than a decision — worth revisiting if takeovers turn out to be
startling at night.

### Severity — resolved, not compromised

`CMD_BRIGHTNESS` is **global per panel**; it cannot make one zone brighter than
another. Per-zone intensity requires the greyscale path. That turns out to
resolve the apparent conflict rather than force a tradeoff, because the two
channels are orthogonal and compose **[measured]**:

- **Global brightness** = base level, follows screen brightness.
- **Per-pixel greyscale** = relative emphasis *within* the frame.

So severity needs no headroom above the base and no pulse — an alerting zone
simply renders at a higher greyscale value than its neighbours. That is also a
persistent state rather than a flash, which is what Q6 wanted from alerts.

## Rendering [measured]

| path | per frame | rate |
|---|---|---|
| `CMD_BRIGHTNESS` (global) | 13.9 ms | 72/s |
| `DrawBW` full frame, 1-bit | 25.3 ms | 39 fps |
| single `StageCol` | 16.9 ms | 59/s |
| greyscale full frame (9 × `StageCol` + `FlushCols`) | 169 ms | 5.9 fps |

(An earlier 430ms greyscale figure was unwarmed, taken during the wake fade.
169ms is the warmed number; design against that.)

**Ambient frames render as greyscale.** They change rarely — clock once a
minute, battery glacially, limits on a 60s poll — so 169ms is imperceptible
when it happens, and greyscale buys per-zone severity emphasis, the context
number's idle/working distinction, and **sub-level bar resolution** (a partial
-intensity top row roughly doubles effective granularity, so the 27-row limit
bars read more like 54 levels).

**Takeovers render as 1-bit `DrawBW`** — 25ms, where latency is the whole
point. **Transitions use global brightness steps** — 14ms each, smooth.

Smooth *animation* is not available (5.9fps on greyscale) and the design does
not require any: severity is static, transitions ride global brightness, and
full on/off gets the firmware's own fade for free via `Sleep`.

### Takeover arbitration

**One takeover slot per panel** — what is shown, and when it expires. Not a
queue, not a stack. Any new event overwrites both fields, and **the timer is
measured from the last event, not the first**, so holding a key keeps the bar up
continuously and it lingers exactly 2s after release.

Queuing was rejected on a category error: a takeover is feedback about *current
state*, not a message. Six volume taps are one thing whose value changed six
times; a queue would spend twelve seconds showing values that are no longer
true.

The ambient frame keeps updating underneath while hidden, so returning from a
takeover is an instant 25ms `DrawBW` of already-current content rather than a
169ms greyscale render at the worst moment.

## Robustness

Device loss (suspend, brownout, unplug) is handled by **udev `tty` add/remove**
for `32ac:0020`, with **write errors (`ENODEV`/`EIO`) as a backstop** — a device
can wedge without emitting a udev event. udev is authoritative and immediate;
without it a dead panel would only be discovered at the next keepalive up to
30s later, and any takeover fired in that window would silently do nothing.

On reopen: wake, set brightness, redraw from scratch (modules come back blank
and possibly asleep). Restore via `DrawBW` first for an instant repaint, with
the greyscale ambient frame following behind.

**`/dev/ttyACM*` numbering can swap across a suspend cycle** — addressing by USB
path rather than kernel enumeration order is what prevents the entire layout
from silently mirroring itself. **[open]** unverified in practice; not worth a
suspend cycle to test mid-session.

## Language

**Python**, with a possible Rust port once it is built and working. Design
consequence to honour now: keep the render layer a **pure function**
(state → frame bytes), with transport and event plumbing around it. That is the
part that ports mechanically and the part worth unit-testing.

## Claude activity indicator

**The context number's brightness** is the activity light: `EMPHASIS` while
Claude is working, `DATA` otherwise. It costs zero rows. This replaced a
dedicated separator rule that did the same job, which went away with the rules.

That is a deliberate **overload of one element by two variables** — the number's
shape says how full the context window is, its intensity says whether a turn is
in flight. It works because the two are read differently (you resolve a shape,
you notice a change), and because the 1.42× swing is well above the ~1.15 ratio
below which a brightness change stops registering at all.

The cost, accepted knowingly: **the indicator now depends on the context
percentage existing.** The two arrive from *different files* — the status-line
tap writes the percentage, the session hook writes the working state — and
`parse()` is explicitly built to tolerate a payload shape that changes with
Claude Code releases. A renamed `context_window` key would therefore take out
the activity light as collateral damage even though the hook feeding it is still
correct. That failure is visible as a dark space where a number belongs, which
is a symptom you can act on, and the snapshot files under
`$XDG_RUNTIME_DIR/matrixd/sessions/` make it diagnosable in seconds.

Hook edges **[measured]** — all exist and are sufficient:

| event | use |
|---|---|
| `SessionStart` / `SessionEnd` | session liveness; gates the context number |
| `UserPromptSubmit` | turn starts — number bright |
| `Stop` / `StopFailure` | turn ends — number normal |
| `PermissionRequest` / `Notification` | **blocked** — available, deliberately unused for now |

**Two states only, not three.** `PermissionRequest` would distinguish *blocked*
from *finished*, which is arguably the highest-value signal here — but a third
state needs three intensity levels discriminated in absolute terms rather than
as a change. That reads well in a mockup and badly on hardware, and the
available range makes it worse than it sounds: between the floor-legible 180 and
the maximum 255 there is 1.42× total, so three levels would be ~1.13× apart —
under the threshold where a difference registers. The daemon consumes these
hooks either way, so the upgrade path costs nothing to preserve.

The indicator tracks **the same session the number shows** (the newest, per the
data-sources section) — otherwise the panel contradicts itself, showing one
session's context at another session's brightness.

## Left panel details

**Clock is 24-hour**, two stacked 2-digit rows (`HH` over `MM`). 12-hour would
need an AM/PM marker, and there is no width for one.

**AC plug and unplug both earn a takeover.** They are confirmations under the
Q6 taxonomy — you did them, and the panel acknowledges. Unplug matters as much
as plug: it is the moment the battery number starts mattering.

**Charging is indicated by a pulse**, carried on **global brightness across the
whole left panel** rather than greyscale on the battery zone alone. Greyscale
frames cost 169ms, so animating one zone would run the serial link at or near
100% duty purely to breathe, and a takeover would then queue behind an in-flight
frame — up to 169ms of the ~300ms feedback budget. Global brightness is 14ms, so
a 3s breathe costs ~28% duty and worst-case queueing is 14ms.

The cost is that the clock breathes too. That is acceptable: it reads as *the
machine panel* indicating a machine state, and whole-panel breathing has exactly
one meaning assigned to it.

### The pulse must not cross the visibility floor [corrected on hardware]

The first version straddled the base: `base ± 35%`, floored at an amplitude of
3 so that a low base still moved. At the bottom of the screen-brightness range
that base *is* `AMBIENT_FLOOR` (3), so the breathe ran **1 → 6 → 1** — and
global 1 and 2 are below the level at which ambient content is lit at all.

It did not read as a dim pulse. It read as the panel switching on and off,
which is a different message entirely and the wrong one.

The mistake was applying "visibility is a product, not a level" to static
frames and forgetting it applies to every instant of an animation. The rule
restated: **no frame of a pulse may sit below the floor, because below the
floor there is no frame.**

So the window now **brightens from the base** instead of straddling it, and
slides down rather than shrinking when it would exceed 255. The floor is then
carried by a single clamp on `base`, not by a second clamp on the low end —
which turned out to be unreachable code that no test could distinguish.

Amplitude is a share of the base and nothing else. No minimum: what the eye
reads is the **ratio** between the ends of the breathe rather than their
difference, so one proportion serves the whole range, and a minimum amplitude
would only make the bottom of the range the harshest part of it. At 25% the
ratio is 1.20–1.34 everywhere from base 3 to base 255 — roughly a third of the
0.7×-of-base swing it replaced.

## Supervision

**systemd user unit**, `WantedBy=default.target`, `Restart=on-failure`.

The usual Hyprland objection does not apply: this daemon needs no Wayland
environment. Its inputs are serial devices, sysfs, udev, the PipeWire socket,
and one HTTPS call — all reachable from a plain user unit with `XDG_RUNTIME_DIR`
alone. Nothing calls `hyprctl`.

Chosen over `exec_cmd` because three things here will fail and should recover
unattended: the `pactl subscribe` child (dies with PipeWire), the serial fds
(die on every suspend), and the daemon itself.

A user unit can start before the seat session goes active, and the `uaccess` ACL
is not applied until it does — so the first open may return `EACCES`. That is
the same path the robustness backstop already handles.

### Details settled while writing the unit

**It runs out of the checkout** — `WorkingDirectory` points at wherever
`install.sh` was run from, and nothing is copied. `git pull` is therefore the
entire update procedure, and there is no second copy of the code to go stale.
The unit in the repo carries an `@INSTALL_DIR@` placeholder that the installer
substitutes, so the file under `~/.config/systemd/user` is generated, not
edited.

**`ExecStart=/usr/bin/env python3 -m matrixd`** rather than a hardcoded
interpreter path, which differs across distributions. The cost is that the
journal would otherwise label every line `env[1234]`; `SyslogIdentifier=matrixd`
buys the name back.

**`Restart=on-failure`, not `always`.** A clean exit is the daemon being told to
stop, and restarting it then would make `systemctl --user stop` unable to stop
it. `RestartSec=5s` matches the panel retry interval — there is nothing to gain
from returning faster than the hardware becomes available.

**`TimeoutStopSec=10s`** is slack, not an expectation. SIGTERM to both panels
asleep is 21ms **[measured, via systemd]**; the timeout covers only the
pathological case where a greyscale frame is still draining behind the `Sleep`.

The `pactl subscribe` child lands in the service cgroup, so it is killed with
the unit and cannot outlive it as an orphan **[measured: `Tasks: 2`]**.

`install.sh` restarts the service when the unit content changed and leaves it
alone when it did not — `enable --now` starts a stopped service but will not
restart a running one, so without that check a re-run after an edit would
report success while the old code kept running.

## Distribution

Standalone repo `jschraub/fw16-ledmatrix`, MIT. `install.sh` owns the udev rule.

Dotfiles integrates via its existing `delegate` convention — registry item
`matrix` → `install-matrix.sh`, which clones or fast-forwards into
`$MATRIX_DIR` (default `~/code/matrix`) and runs the repo's own installer. Not a
submodule: the project should be installable by people who are not the author.

## Keyboard backlight — abandoned

Tying anything to the FW16 keyboard backlight is not practical. It is
EC-controlled with no `/sys/class/leds` entry, and every route to it is
root-gated **[measured]**:

| route | blocker |
|---|---|
| `framework_tool --kblight` | reads `/sys/firmware/dmi/tables/DMI`, `-r-------- root root`. Also **exits 0 on failure** |
| `ectool` via `/dev/cros_ec` | `crw------- root root`, no udev rule; needs AUR `fw-ectool-git` |
| HID to the keyboard module | `/dev/hidraw*` are `crw------- root root`, no `uaccess` |

Granting `uaccess` on `/dev/cros_ec` looks like the LED-matrix rule but is not a
comparable trade — the EC also controls fans, charging, and firmware paths. A
`NOPASSWD` sudoers entry is worse. Neither is worth a cosmetic idle behaviour.

The dead hypridle listener that targeted `rgb:kbd_backlight` has been removed;
it had been silently failing since it was written, without being missed.

## Render layer

`matrixd/render.py` is pure — no I/O, no sleeps, no hardware. Every function is
a pure function of its arguments, which is what makes the layout testable
without a laptop attached and what makes it the piece that ports to Rust
mechanically. `tools/preview.py` is the only place render meets hardware.

Coordinate conventions **[measured]** — see README for the table. All four were
found by lighting patterns and looking; none are documented upstream.

Relative intensity ladder within a frame: `DATA 180`, `EMPHASIS 255`. Two
levels, both meaningful — there is no decoration tier any more. `DATA` sits
below full scale so emphasis has somewhere to go, and as low as the visibility
floor permits so it has as much room as possible.

Bars carry a **partial-intensity top row** proportional to the remainder,
roughly doubling effective resolution — a 27-row bar reads closer to 54 levels.
Each bar takes an **explicit inclusive column range** rather than a symmetric
inset, because the Claude panel places two bars side by side and no inset can
express that.

The tip has **no floor**. It used to be `max(1, int(value * tip))`, justified as
"so a barely-started bar still shows something" — which it could not do:
greyscale 1 needs a global brightness of 520 to clear the threshold and the
hardware maximum is 255, so the floor guaranteed a pixel invisible on every
setting the panel has. Another instance of the pattern below.

Two-digit values render `XX` when they do not fit, rather than the earlier clamp
to `99`. Both avoid the confidently-wrong `23` for 123; only `XX` also avoids
looking like a reading.

## Open questions

- Whether the brightness curve matches the `-e4` exponential of the keybinds.
- Re-check the intensity ladder at 2/100 screen in a genuinely dark room.

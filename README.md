# fw16-ledmatrix

Status daemon for the Framework Laptop 16 LED Matrix input modules — drives the
two 9×34 panels flanking the keyboard as ambient displays.

**Left panel = machine** (time, battery). **Right panel = Claude Code** (5-hour
and weekly rate limits, context usage). Values that you change rather than
watch — volume, screen brightness — get no permanent space; they take over a
panel for ~2s at the moment you change them, then it returns to ambient.

> **Status: in progress.** The render layer, serial transport, and most data
> sources are built and verified against real hardware; the event loop that ties
> them together is not written yet, so nothing runs unattended. See
> [STATUS.md](STATUS.md) for exactly where things stand and what is next, and
> [DECISIONS.md](DECISIONS.md) for why anything is the way it is.

## Install

```sh
git clone https://github.com/jschraub/fw16-ledmatrix ~/code/matrix
cd ~/code/matrix && ./install.sh
```

`install.sh` installs a udev rule (needs `sudo`) granting the active-seat user
access to the modules, and drops two Claude Code integration scripts into
`~/.claude`. Nothing else requires root, and there are no Python dependencies —
the transport is raw `termios`, no `pyserial`.

Pass `--no-claude` to skip the Claude Code half, `--dry-run` to see what it
would do, or `--uninstall` to remove it all again.

## Claude Code integration

The right panel shows Claude Code's rate limits and context usage. Rate limits
work out of the box, but **context percentage needs two scripts wired into your
Claude Code config** — it is piped to the status line and exposed nowhere else,
because it is a property of a live conversation rather than of your account.
There is no file on disk to read and no API to ask.

`install.sh` copies both into `~/.claude` and prints the exact JSON to add. They
do nothing until `settings.json` refers to them:

| script | supplies |
|---|---|
| `matrix-statusline-tap.sh` | context %, rate limits — the values |
| `matrix-session-hook.sh` | whether a turn is in flight — the edges |

Two scripts because the status line renders on a timer, so it can report a
percentage but cannot say when Claude starts and stops working.

**The tap wraps your status line, it does not replace it.** Claude Code allows
only one `statusLine` command, so the tap takes the payload, snapshots it, and
hands it to your command on stdin unchanged:

```json
"statusLine": {
  "type": "command",
  "command": "~/.claude/matrix-statusline-tap.sh ~/.claude/your-statusline.sh"
}
```

Drop the trailing argument if you do not have a status line. If you would rather
your status line never depend on this being installed, guard it:

```json
"command": "[ -x ~/.claude/matrix-statusline-tap.sh ] && exec ~/.claude/matrix-statusline-tap.sh ~/.claude/your-statusline.sh || exec ~/.claude/your-statusline.sh"
```

`install.sh` does **not** edit `settings.json` itself. It is your file, it can
contain anything, and mangling it would be a poor trade for saving you a paste.

The two halves meet at a directory rather than at code, so a missing piece
degrades instead of breaking:

```
$XDG_RUNTIME_DIR/matrixd/sessions/<session-id>.json     values
$XDG_RUNTIME_DIR/matrixd/sessions/<session-id>.state    "working" | "idle"
```

One file per session because several Claude Code sessions can be open at once;
the daemon shows whichever rendered most recently. `XDG_RUNTIME_DIR` is tmpfs
and clears on logout, so a session killed hard enough to skip `SessionEnd`
cannot strand a frozen percentage on the panel.

Skip all of this and everything except the Claude panel still works.

## Try it

```sh
tools/smoke.py probe              # firmware version on each panel; changes nothing
tools/smoke.py sweep              # light each panel in turn — tells you which bay is which
tools/smoke.py on left            # fill one panel
tools/smoke.py hold left 1 25     # hold a brightness level to judge it
tools/smoke.py ramp left          # step the brightness range to calibrate by eye
tools/smoke.py off all
```

## Hardware notes

Findings from characterising the modules, all measured rather than assumed.
These cost real time to discover, so they are written down here in the hope
they save someone else the same afternoon.

**Both modules report the same USB serial number** (`FRAKDEBZ0100000000`), so
`/dev/serial/by-id/` collapses to a single symlink and cannot distinguish left
from right. Enumerate via `/dev/serial/by-path/` instead — USB topology is
stable per physical bay, and it also survives `ttyACM*` renumbering across
suspend, which would otherwise silently mirror your layout.

**Nothing in USB topology reveals which bay is which.** You have to light one
and look. On this machine, USB port `3.3` is the *right* bay and `4.2` is the
*left* — note the inversion, the lower-numbered port is the right-hand side.
Verify yours with `tools/smoke.py sweep`.

**The firmware sleeps on an idle timer** (default 60s, reset by any command).
A sleeping module does not answer: the first command wakes it and is consumed
doing so, and waking fades the LEDs in over a period during which commands are
not serviced. A bare version query after an idle period reliably returns zero
bytes, which looks exactly like a broken device. Send an explicit wake
(`Sleep 0`) first, allow ~0.4s, and retry the query.

The corollary matters for any always-on display: **you must send traffic more
often than the idle timer** or the firmware will blank your panels for you.

**Opening the port costs ~0.2s** before the device will accept a command
(CDC-ACM line-state settling). Hold ports open for the process lifetime rather
than reopening per frame.

**Coordinate conventions**, none of which are documented anywhere and all of
which were found by lighting patterns and looking:

| | |
|---|---|
| `y = 0` | far end of the panel, toward the screen |
| `y = 33` | near end, toward you |
| `x = 0` | your left — on **both** panels; the modules are seated alike, so no per-panel mirroring |
| `DrawBW` packing | row-major, `bit = y*9 + x`, **LSB-first** within each byte |

Getting any of these backwards renders mirrored, transposed, or upside down
while looking perfectly plausible in code, so they are worth five minutes with
`tools/preview.py` on your own machine rather than trusting this table.

**Visibility is a product, not a level.** An LED reads as lit when
`global_brightness × greyscale ≳ 520`. Neither number matters on its own —
digits at greyscale 200 are legible at global 3 but not 2 (600 vs 400), and
rules at greyscale 60 are legible at global 9 but not 8 (540 vs 480).

A corollary that costs people time: **calibrate against representative content,
never a solid fill.** A full panel lights all 306 LEDs and reads as a glow at
currents where a few thin bands are completely invisible. A floor derived from a
fill will be far too low for a real frame.

**Command timings** (measured, warmed):

| path | per frame | rate |
|---|---|---|
| `Brightness` (global, 1 byte) | 13.9 ms | 72/s |
| `DrawBW` full frame (39 bytes, 1-bit) | 25.3 ms | 39 fps |
| single `StageCol` (35 bytes) | 16.9 ms | 59/s |
| greyscale full frame (9 × `StageCol` + `FlushCols`) | 169 ms | 5.9 fps |

So 1-bit drawing is effectively free and greyscale is not — smooth animation is
off the table, but static greyscale content that changes rarely costs nothing
you can perceive. `Brightness` is **global per panel**; per-zone intensity
requires the greyscale path. The two compose: global brightness scales the
greyscale values rather than overriding them.

**Writes do not block, so those timings are not what your caller experiences.**
The tty buffers them: a write returns in ~0.1ms while the data drains in the
background at the rates above. A frame is therefore *not* on screen when the
write returns, and anything written behind a queued greyscale frame is delayed
by up to 165ms — so a takeover issued mid-frame appears late even though its own
write costs 25ms. Use `tcdrain()` if you need to know it landed.

**`TIOCOUTQ` does not report bytes on `cdc-acm`.** It reports
`writesize × URBs in flight` — measured at 1280 per outstanding write, so a
greyscale frame (which is ten separate writes) reads 12800 for ~345 bytes of
payload, and it saturates at 16 URBs (20480) however much more you queue. Useful
as a busy/idle signal, useless as a byte count.

Do **not** flush the output queue to make a takeover jump the line: truncating a
`StageCol` mid-payload leaves the module's command parser consuming your next
command as payload bytes. A late frame is much cheaper than a desynced parser.

## Protocol

USB CDC-ACM, 115200 8N1. Every command is `0x32 0xAC` then a command byte then
its payload.

| command | id | payload | response |
|---|---|---|---|
| Brightness | `0x00` | 1 byte | — |
| Sleep | `0x03` | 1 byte (or none to query) | — (1 byte if querying) |
| DrawBW | `0x06` | 39 bytes (9×34 bits) | — |
| StageCol | `0x07` | 1 byte column + 34 bytes | — |
| FlushCols | `0x08` | — | — |
| Version | `0x20` | — | 3 bytes (bcdDevice MSB, LSB, pre-release flag) |

Version bytes are **BCD**: `00 20 00` is firmware 0.20, not 0.32.

Full protocol: [FrameworkComputer/inputmodule-rs](https://github.com/FrameworkComputer/inputmodule-rs).

## Design

[DECISIONS.md](DECISIONS.md) records the design and, more usefully, the
reasoning and the rejected alternatives.

## License

MIT

# fw16-ledmatrix

Status daemon for the Framework Laptop 16 LED Matrix input modules — drives the
two 9×34 panels flanking the keyboard as ambient displays.

**Left panel = machine**: the time as two stacked 2-digit rows, then a battery
bar. **Right panel = Claude Code or OpenCode/OpenAI**: the context percentage as a number, then the
5-hour and weekly rate limits as two bars side by side. The number brightens
while the selected session is working, and reads `XX` at 100%.

Values that you change rather than watch — volume, screen brightness — get no
permanent space; they take over a panel for ~2s at the moment you change them,
then it returns to ambient.

> **Status: it works and it starts on login.** Every layer is built and verified
> against real hardware. What it has not had yet is a long uninterrupted run.
> See [STATUS.md](STATUS.md) for exactly where things stand and
> [DECISIONS.md](DECISIONS.md) for why anything is the way it is.

## Install

```sh
git clone https://github.com/jschraub/fw16-ledmatrix ~/code/matrix
cd ~/code/matrix && ./install.sh
```

That installs three things: a udev rule granting the active-seat user access to
the modules (the only step needing `sudo`), a systemd **user** service that runs
the daemon on login, and two Claude Code integration scripts in `~/.claude`.
There are no Python dependencies — the transport is raw `termios`, no
`pyserial`.

Pass `--no-service` or `--no-claude` to skip a part, `--dry-run` to see what it
would do, or `--uninstall` to remove it all again.

For an OpenAI subscription authenticated through OpenCode:

```sh
./install.sh --provider opencode-openai
```

This also installs the optional OpenCode session plugin and configures the user
service to use `opencode-openai`. **Quit and restart OpenCode** to load the
plugin. Pass `--no-opencode` to skip plugin installation (quota bars still work).
The provider defaults to `claude`; repeat `--provider opencode-openai` when
re-running the installer to keep that service selection.

The service runs the code from the directory you installed from and nothing is
copied, so updating is just:

```sh
cd ~/code/matrix && git pull && ./install.sh
```

## Run it

```sh
systemctl --user status matrixd          # is it up
journalctl --user -u matrixd -f          # what is it doing
systemctl --user restart matrixd
```

Or by hand, which is the better way to watch it:

```sh
python3 -m matrixd        # left: clock + battery; right: context% + rate limits
python3 -m matrixd -v     # log takeovers and panel connect/disconnect
python3 -m matrixd --provider opencode-openai  # OpenCode session + OpenAI quotas
python3 -m matrixd --provider claude          # Claude (also the default)
```

Stop the service first — two instances will both write to the same panels.

Ctrl-C or `SIGTERM` puts both panels to sleep on the way out, measured at 21ms.
Idle cost is one wakeup a second and no measurable CPU.

To switch the login service, re-run `./install.sh --provider opencode-openai`
or `./install.sh --provider claude`. The installer updates the startup flag and
restarts the service when its unit changes. Selection is per daemon process,
not a live control command.

## OpenCode / OpenAI integration

Sign in to **OpenAI with your ChatGPT subscription in OpenCode** first. The
`opencode-openai` name identifies both the client supplying credentials and the
subscription provider; it does not use Codex CLI's login.

**Quota bars come directly from OpenAI**, polled every 60 seconds using
`GET https://chatgpt.com/backend-api/wham/usage`. The daemon reads the `openai`
OAuth entry in `$XDG_DATA_HOME/opencode/auth.json` (normally
`~/.local/share/opencode/auth.json`) on each poll. It sends the access token and
the account ID when present. It never writes credentials or refreshes tokens;
OpenCode owns that. API-key logins do not supply subscription quotas.

`OPENCODE_AUTH_CONTENT` takes precedence if set in the daemon's environment.
If you use custom XDG paths or environment-provided auth, the daemon/service
must receive the same environment as OpenCode. A shell's environment is not
automatically inherited by an already-running systemd user service.

The bars show **percentage used**, five hours on the left and seven days on the
right. Windows are matched by their actual duration, not by the endpoint's
primary/secondary ordering. Missing windows stay blank; model-specific buckets,
credit balances, and other ChatGPT feature limits are not mapped onto these
bars. This is an internal endpoint and its format can change. Failed requests
retain the last reading for up to five minutes after its successful poll; then
the bars go blank. Changing/logging out of the account clears cached readings
on the next poll.

**Context and activity come from the OpenCode plugin** at
`integration/opencode/matrix-session.js`. The installer copies it into
`$XDG_CONFIG_HOME/opencode/plugins/` (normally `~/.config/opencode/plugins/`).
OpenCode auto-loads it on startup; no config JSON edits or npm dependencies are
needed. Manual installation:

```sh
install -D -m 0644 integration/opencode/matrix-session.js \
  "${XDG_CONFIG_HOME:-$HOME/.config}/opencode/plugins/matrix-session.js"
```

Quit and restart OpenCode after installing or updating the plugin, then send a
prompt. Sessions become visible when they produce events in that running
instance; historical conversations are not loaded from the database. Context
becomes available after an assistant response reports token usage. It uses the
same latest-response token calculation and rounding as OpenCode's context
sidebar, including cached and reasoning tokens and the model's context limit.

The panel follows the main OpenAI conversation with the latest user-message or
assistant activity across local OpenCode instances. Child/subagent sessions are
excluded. This follows activity, not terminal focus. Busy/retrying brightens the
number; idle/error returns it to normal brightness.

The plugin writes only session IDs, provider ID, context percentage, activity
state, and activity timestamps. No conversation text or credentials are copied:

```text
$XDG_RUNTIME_DIR/matrixd/opencode/<instance-id>.json
```

Snapshots are written atomically with mode `0600`. A 15-second heartbeat keeps
idle-but-open sessions visible without changing their activity ordering. After
one minute without a heartbeat the context/activity zone goes blank. Instance
disposal removes its snapshot; crashes are handled by heartbeat expiry. With a
persistent OpenCode server, health means that server instance is alive, not that
a particular attached terminal is still open.

Without the plugin or `XDG_RUNTIME_DIR`, the quota bars still work independently.
The Claude feed uses a separate directory and retains its existing behavior.

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

Tests without LED hardware:

```sh
python3 -m unittest discover -s tests -t .
node --test tests/test_opencode_plugin.mjs
```

The plugin tests use Node's built-in test runner; OpenCode runs the installed
plugin using its own JavaScript runtime.

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
(`Sleep 0`) first, allow **1s**, and retry the query.

**That settle needs to be a full second, and 0.4s is a trap** — long enough for
a command to be accepted, so it looks like it works, but not for the link to be
back at full speed. Draining a greyscale frame written at varying delays after
a wake:

| settle | drain | | settle | drain |
|---|---|---|---|---|
| 0.0s | 654 ms | | 1.0s | 165 ms |
| 0.4s | 253 ms | | 5.0s | 165 ms |

165ms is the steady-state figure, so anything past 1s buys nothing and anything
under it makes the first frames after a wake crawl. This is specifically the
fade, not a cold link: with no traffic at all for 1, 5, 15, 30 and 45 seconds,
drains stayed at 130-165ms throughout.

Worth knowing because it hides well. It surfaces as *shutdown* being slow, or a
panel being sluggish only right after a resume — never as an error.

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
digits at greyscale 200 are legible at global 3 but not 2 (600 vs 400), and a
1px rule at greyscale 60 is legible at global 9 but not 8 (540 vs 480).

Note that 520 is *interpolated* between those two observations, not measured.
The dimmest product actually seen to be legible is 540, which is why this
project's `DATA` level is 180 (540 at the brightness floor) rather than the 174
the arithmetic would allow.

A corollary that costs people time: **calibrate against representative content,
never a solid fill.** A full panel lights all 306 LEDs and reads as a glow at
currents where a few thin bands are completely invisible. A floor derived from a
fill will be far too low for a real frame.

A second corollary, which cost this project a bug: **the floor applies to every
frame of an animation, not just to static ones.** A brightness pulse that dips
below it does not look dim, it looks like the panel switching off — and since
the floor is where the panel sits at low screen brightness, that is exactly
where a pulse centred on the current level will land.

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

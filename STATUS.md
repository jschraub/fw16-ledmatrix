# Where this is up to

Last touched 2026-08-07. Read this first, then [DECISIONS.md](DECISIONS.md) for
*why* anything is the way it is.

Everything below is committed and pushed. 74 tests pass (`python3 -m unittest
discover -s tests -t .`). The daemon does not exist yet — there is no event loop
and nothing runs unattended; what exists is every layer beneath it, each verified
against real hardware.

## Built

| | what it does | verified |
|---|---|---|
| `matrixd/render.py` | pure `state → frame`; layouts, 3×5 font, bars, both encoders | on hardware via `tools/preview.py` |
| `matrixd/transport.py` | protocol, connection lifecycle, keepalive, reconnect | live round-trip on both panels |
| `matrixd/sources/usage.py` | Claude 5h/7d from the OAuth endpoint | live: returned real percentages |
| `matrixd/sources/power.py` | battery %, charging, AC | live |
| `matrixd/sources/screen.py` | screen brightness → panel brightness | live |
| `matrixd/sources/udev.py` | netlink watcher: `tty` + `power_supply` | parses real captured messages |
| `install.sh` | udev `uaccess` rule | installed and working |

Host integration is done: the udev rule is installed, and dotfiles has an
`install-matrix.sh` plus registry item `matrix` that clones or fast-forwards
`~/code/matrix` and runs this repo's installer.

## Next, in dependency order

1. **`sources/audio.py`** — volume via a long-lived `pactl subscribe` child.
   Needs supervised respawn: it dies whenever PipeWire restarts, and if it stays
   dead volume takeovers silently stop working.
2. **`sources/claude_session.py`** — context % and session liveness. The only
   piece that writes outside this repo: it needs a shim appended to
   `~/.claude/statusline.sh` (dump the stdin JSON to a snapshot file) and hook
   entries in `~/.claude/settings.json` for `SessionStart`, `SessionEnd`,
   `UserPromptSubmit`, `Stop`, `StopFailure`.
3. **The event loop** — one epoll over the udev fd, the `pactl` child, the
   brightness keybind socket, and timers for the clock, the 60s usage poll, and
   the 30s keepalive.
4. **systemd user unit** — `WantedBy=default.target`, `Restart=on-failure`.
   Deliberately *not* tied to `graphical-session.target`: this daemon needs no
   Wayland environment, so it avoids the usual Hyprland unit plumbing entirely.

## Known gaps and risks

**The panel disconnect/reconnect path has never actually run.** It is the most
likely place for a latent bug. Unit tests cover the parse and filter halves
only — the modules are internal so they cannot be unplugged, and pty allocation
emits no `tty` uevents. `tools/test-reconnect.py` closes this by deauthorising
the USB device, producing the same remove/add pair a suspend cycle does:

    sudo python3 tools/test-reconnect.py

It has been written and its USB-node resolution verified, but **it has not been
run**. Do this first when picking back up.

**Two open design questions**, neither blocking:

- Whether the screen→panel brightness curve should match the `-e4` exponential
  of the hypr keybinds. Currently linear between the two calibrated anchors.
  Better settled by looking at it than reasoning about it.
- Whether takeovers need their own brightness offset. A full-height gauge is
  close to a solid fill while an ambient frame is mostly dark, so at equal
  global brightness a takeover emits far more light. Probably desirable for
  something meant to grab attention — but it is a consequence of coverage, not a
  decision, and may be startling at night.

**A correction worth not re-making:** the brightness floor was originally
recorded as 1/255, measured by ramping a *solid fill* and then applied to sparse
content. It is 3. Calibrate against representative frames, never a fill — a full
panel reads as a glow at currents where a few thin bands are invisible.

## Things that will bite you

Written down because each one cost real time to find:

- The firmware sleeps after ~60s idle. A sleeping module answers nothing, and
  the first command is consumed waking it — a cold version query looks exactly
  like a dead device.
- Writes do not block. `draw()` returns in ~0.1ms while the data drains for up
  to 165ms, so a frame is not on screen when the call returns.
- `TIOCOUTQ` reports `writesize × URBs`, not bytes. Use `is_busy()`.
- Both modules share one USB serial number. Address by `/dev/serial/by-path/`.
- The lower-numbered USB port is the **right** bay.
- `bcdDevice` is BCD: `0x00 0x20` is firmware 0.20, not 0.32.
- The usage endpoint puts severity in `limits[]`, which names the windows
  `session` and `weekly_all` — matching neither top-level key.
- Rules are invisible at the brightness floor. **This is intentional**; see the
  comment block in `render.py` before "fixing" it.

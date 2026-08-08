# Where this is up to

Last touched 2026-08-07. Read this first, then [DECISIONS.md](DECISIONS.md) for
*why* anything is the way it is.

Everything below is committed and pushed. 95 tests pass (`python3 -m unittest
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
| `matrixd/sources/audio.py` | volume + mute, supervised `pactl subscribe` | live: real changes seen, beeps ignored |
| `install.sh` | udev `uaccess` rule | installed and working |

The disconnect/reconnect path **has now been run** — `tools/test-reconnect.py`
passed end to end: udev reported the removal, the write backstop raised
`PanelGone`, and the panel reconnected and repainted. That was the largest
unverified assumption in the project.

Host integration is done: the udev rule is installed, and dotfiles has an
`install-matrix.sh` plus registry item `matrix` that clones or fast-forwards
`~/code/matrix` and runs this repo's installer.

## Next, in dependency order

1. **`sources/claude_session.py`** — context % and session liveness. The only
   piece that writes outside this repo: it needs a shim appended to
   `~/.claude/statusline.sh` (dump the stdin JSON to a snapshot file) and hook
   entries in `~/.claude/settings.json` for `SessionStart`, `SessionEnd`,
   `UserPromptSubmit`, `Stop`, `StopFailure`.
2. **The event loop** — one epoll over the udev fd, the `pactl` child, the
   brightness keybind socket, and timers for the clock, the 60s usage poll, and
   the 30s keepalive. Note the audio fd is not stable: it changes across a
   respawn and is -1 while there is no child, so it must be re-registered rather
   than registered once (`Subscriber`'s docstring has the loop shape).
3. **systemd user unit** — `WantedBy=default.target`, `Restart=on-failure`.
   Deliberately *not* tied to `graphical-session.target`: this daemon needs no
   Wayland environment, so it avoids the usual Hyprland unit plumbing entirely.

## Known gaps and risks

**Nothing has run for hours yet.** Every component has been exercised, but only
in short bursts. The failure modes that need real uptime — firmware idle sleep
across a whole day, a suspend/resume cycle, an OAuth token expiring — are
untested by construction until the daemon exists.

Re-run the reconnect check after any change to `transport.py` or `sources/udev.py`;
it needs root because it deauthorises the USB device to produce a genuine
remove/add pair:

    sudo python3 tools/test-reconnect.py

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
- A PulseAudio sink event does **not** mean the volume changed — playing any
  sound emits two of them. Re-read and compare, or every notification pops a
  volume takeover.

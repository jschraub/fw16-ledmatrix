# Where this is up to

Last touched 2026-08-08. Read this first, then [DECISIONS.md](DECISIONS.md) for
*why* anything is the way it is.

Everything below is committed and pushed. 162 tests pass (`python3 -m unittest
discover -s tests -t .`).

**The daemon runs.** `python3 -m matrixd` drives both panels from live state.
What is missing is only the service unit, so it does not start on login yet.
Measured over a 24s run: 0% CPU, 6MB RSS, and SIGTERM to both-panels-asleep in
22ms.

## Built

| | what it does | verified |
|---|---|---|
| `matrixd/render.py` | pure `state → frame`; layouts, 3×5 font, bars, both encoders | on hardware via `tools/preview.py` |
| `matrixd/transport.py` | protocol, connection lifecycle, keepalive, reconnect | live round-trip on both panels |
| `matrixd/sources/usage.py` | Claude 5h/7d from the OAuth endpoint | live: returned real percentages |
| `matrixd/sources/power.py` | battery %, charging, AC | live |
| `matrixd/sources/screen.py` | brightness → panel level, DPMS off, auto-change marker | live |
| `matrixd/sources/udev.py` | netlink watcher: `tty`, `power_supply`, `backlight` | parses real captured messages |
| `matrixd/sources/audio.py` | volume + mute, supervised `pactl subscribe` | live: real changes seen, beeps ignored |
| `matrixd/sources/claude_session.py` | context %, session liveness | live: read a real running session |
| `matrixd/daemon.py` | the event loop — one epoll, all inputs, both panels | live: every takeover path driven end to end |
| `install.sh` | udev rule + Claude scripts | installed and working |

The disconnect/reconnect path **has now been run** — `tools/test-reconnect.py`
passed end to end: udev reported the removal, the write backstop raised
`PanelGone`, and the panel reconnected and repainted. That was the largest
unverified assumption in the project.

Host integration is done: the udev rule is installed, and dotfiles has an
`install-matrix.sh` plus registry item `matrix` that clones or fast-forwards
`~/code/matrix` and runs this repo's installer.

**The Claude session feed has a producing half that ships in this repo**, under
`integration/claude/` — a status line tap and a hook script, installed into
`~/.claude` by `install.sh`. They were briefly kept in dotfiles instead, which
was wrong: anyone cloning this repo would have got a Claude panel that silently
did nothing, and the snapshot logic would have existed in two places. dotfiles
now just points `settings.json` at the installed copies.

The consuming half is `sources/claude_session.py`. The two meet at a directory
layout, not at code, so a missing piece degrades rather than breaking:

    $XDG_RUNTIME_DIR/matrixd/sessions/<session-id>.json     values
    $XDG_RUNTIME_DIR/matrixd/sessions/<session-id>.state    "working" | "idle"

Context percentage is available *only* from the status line payload — no file on
disk holds it and no API reports it, because it is a property of a live
conversation rather than of an account.

## Next, in dependency order

1. **systemd user unit** — `WantedBy=default.target`, `Restart=on-failure`.
   Deliberately *not* tied to `graphical-session.target`: this daemon needs no
   Wayland environment, so it avoids the usual Hyprland unit plumbing entirely.
   `install.sh` should install and enable it. This is the only thing standing
   between "runs when you start it" and "just works".
2. **Then leave it running for a day** — see below.

Both open design questions below are about how the panels *look*, so they are
best answered while it is running rather than before.

## Known gaps and risks

**Nothing has run for longer than a minute.** Every path has been exercised, but
in bursts of seconds, and the remaining failure modes are the ones that need
real uptime and cannot be tested any faster than they happen:

- a suspend/resume cycle, which drops and re-enumerates both panels at once
- an OAuth access token expiring (~12h), after which usage should go stale
  rather than wrong, and recover when Claude Code renews it
- PipeWire restarting under the volume subscriber
- the firmware idle timer across a whole day of mostly-static frames

None of these are suspected broken; they are simply unobserved. Watching
`python3 -m matrixd -v` across a normal day is the test.

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
**And the sequel to it:** the charging pulse then straddled the base level, so
at low screen brightness — where the base *is* the floor — it ran 1 → 6 → 1 and
the panel visibly blinked rather than breathed. The floor is a property of every
frame, including the frames of an animation. Fixed by brightening upward from
the base instead of straddling it.


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
- A session id from the status line payload becomes a path component, and
  `SessionEnd` feeds it to `rm -f`. Both producing scripts validate it: an id of
  `../../escape` was confirmed to write outside the directory without the guard.
- **Writing during the post-wake fade is slow, and looks like nothing.** At the
  old 0.4s settle a greyscale frame took 253ms to drain instead of 165ms, and at
  0s it took 654ms. `WAKE_SETTLE` is now 1.0s. It showed up as a two-second
  shutdown, not as an error.
- An absolute monotonic deadline initialised to `0.0` is not "do it now", it is
  permanently overdue — every poll returns immediately and the loop spins at
  100% of a core while looking perfectly healthy. Seed deadlines from the clock.
- A respawned child process usually gets the dead one's fd number back, and
  closing an fd silently drops it from the epoll set. An unchanged fd number
  therefore does **not** mean it is still registered.

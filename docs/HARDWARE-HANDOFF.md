# Hardware-session handoff

Updated **2026-11-24** after owner review of all four Signal Room workspaces.
Repository: [jschraub/fw16-ledmatrix](https://github.com/jschraub/fw16-ledmatrix).

**Current checkpoint: Linux feedback overlays (#27) and packaging/login startup
(#28) complete. Account-bound usage (#22) passed the packaged OpenAI check;
one-way migration and Rust diagnostics (#29) are complete.** Windows
development (#42) and support qualification (#43) are post-v1. Linux optical
lock, display-off and an attended system suspend/resume cycle passed. Details are in
[SUPPORT.md](SUPPORT.md#initial-cachyos-hardware-session--2026-11-23).
Physical module-removal testing is paused because it interrupted keyboard and
touchpad input. Full-day soak and Windows bay qualification remain pending.

## Resume this machine

- `fw16-ledmatrix.service` is the **enabled, active systemd user service** from
  package `0.1.0-4`. The installed `/usr/bin/matrix-daemon` resumed revision 35
  and alone owned both ports. The temporary `90-preview-fix.conf` was removed;
  the only remaining drop-in enables OpenAI polling and leaves Claude off.
- The review desktop unit was stopped. The **Matrix Studio** launcher opened
  `/usr/bin/matrix-desktop` and the packaged panel map connected successfully.
- The legacy `matrixd.service` is disabled and inactive. The transient handoff
  daemon was stopped before starting the installed service. Private POC backups
  and the handoff config remain in `$HOME/.local/share/fw16-matrix-handoff/`.
- Launch **Matrix Studio** from the desktop menu or `/usr/bin/matrix-desktop`
  without handoff environment overrides. The installed config is now
  `~/.config/fw16-matrix/config.json`; the endpoint is `$XDG_RUNTIME_DIR/fw16-matrix/daemon.sock`.
  Do not launch a second daemon with the old physical bindings.

```sh
systemctl --user show matrixd.service fw16-ledmatrix.service fw16-matrix-handoff-daemon.service --property=Id,ActiveState,UnitFileState,MainPID
fuser -v /dev/ttyACM{0,1}
```

Keep the POC service disabled. The owner designated it reference-only. The
installed Rust service is the recovery target; see [MIGRATION.md](MIGRATION.md).

## Immediate test and session results

1. Existing-account graphical login passed: the owner saw the clock and battery
   gauge return after logout/login. The packaged daemon had a new PID, served
   revision 26 on default IPC and alone held both firmware-0.20 ports.
2. Fresh-user login was not tested; the owner accepted the existing-account
   login and staged package transactions for #28. Leave installed startup enabled.

| Area | Evidence from this session |
| --- | --- |
| #31 capability screen | Native two-panel report matched CLI; zero-panel report and unreachable-daemon error passed using isolated instances. |
| #32/#37 optics | Left numeric layout showed legible overflow `XX` at 100% battery and correct gauge placement. Right sparse clock at brightness 3/intensity 180 was dim but readable. Both preview checks restored saved output. |
| #38 preview ownership/restoration | Competing client rejected; owner EOF released lease. Isolated expiry/cancel/owner-kill restored concurrent Apply. On hardware the left clock appeared after expiry, rollback restored its battery gauge, and the right clock stayed visible. The 1.99 s acknowledgment includes human reaction; it is not LED latency. |
| #26 display/session | Right clock visibly followed screen 55% → 25%; left override held. Owner confirmed per-panel locked roles, DPMS sleep/wake, and both roles and input after system resume. Linear/quadratic mapping, manual fallback, monitor restart and latest-output restoration tested in fixtures. Windows remains unqualified. |
| #41 product design | Owner selected **Signal Room** from the [three design studies](https://github.com/jschraub/fw16-ledmatrix/tree/prototype/41-design-studies) and responded positively to the [four redesigned workspaces](design/workspace-audit.md). Native release sign-off pending. |

Repeatable Linux fixtures (after building `matrix-daemon`): `python3
tools/check-display-follow.py`, `python3 tools/check-lifecycle-signals.py`,
`python3 tools/check-lock-content.py`, `python3 tools/check-suspend-resume.py`, and `python3 tools/check-preview-apply.py`.
See [WIDGETS.md](WIDGETS.md) for actual lock polling, fallback and priority limits.

## Ongoing: capture the unresolved serial failure (#34 / #36)

1. Read the failure sequence and successful checks in SUPPORT.md. Both panels
   reported firmware 0.20 and were visually identified. Initial attempts showed
   stale POC content instead of an outline, then reconnecting/write-quarantine
   status. No failing per-command trace has been captured.
2. Startup partial-write quarantine was reproduced and fixed (`e1ca58d`).
   The resumed investigation now has Linux PTY backpressure tests and failure-only
   serial-boundary diagnostics; see [SERIAL-FAILURE-INVESTIGATION.md](SERIAL-FAILURE-INVESTIGATION.md).
   These reproduce injected write timeouts, not the unexplained physical failure.
3. Compare Rust and POC at the serial boundary. Rust's 250 ms timeout applies to
   writes as well as reads; POC writes block. This is a lead, not a confirmed
   cause or justification for simply increasing the timeout.
4. Preserve partial-write quarantine. Restarting the daemon clears its in-memory
   state but does not prove the firmware parser is reset. The right panel
   succeeded on a retry after a daemon restart **without** physical reconnection;
   the left succeeded after an explicit five-second disconnect/reconnect. Neither
   result establishes automatic recovery reliability.
5. Keep both modules installed for now. During a later removal attempt, keyboard
   and touchpad stopped working; reconnecting the left module restored input and
   both displays. Establish a supported removal procedure and independent input
   before resuming physical-removal tests. The mechanism is still unknown.

The handshake now allows three unanswered version queries with two-second reply
windows and 300 ms gaps, retaining partial replies within each window. Six new
regression tests cover missing/fragmented replies and bounded failure behavior.
**This did not eliminate the observed write failure.** Limits are documented in
WIDGETS.md. Keep #34/#36 and their parent specs open.

## Firmware release check

Checked Framework's official repository on **2026-11-23**: the latest published
LED Matrix firmware is [v0.2.0, released 2023-12-06](https://github.com/FrameworkComputer/inputmodule-rs/releases/tag/v0.2.0).
Both panels' **0.20** readings correspond to v0.2.0: the USB version encoding
packs minor and patch digits together, and the current app displays those digits
without a separating dot. This does not fingerprint the installed binary.

The [development branch has newer source changes](https://github.com/FrameworkComputer/inputmodule-rs/compare/v0.2.0...main),
but no newer published firmware release was found. No firmware was flashed.
Keep the current firmware for reproducible host-side diagnosis; a development
build is not an established fix for these failures.

## Current state and authoritative references

Software slices #9–#21 and #23–#25 are closed with individual commits and
validation records. Their closure records software delivery, **not physical
qualification**. The original development machine lacked panels and native-window
interaction; this session added limited CachyOS/Hyprland hardware evidence in
SUPPORT.md, not full platform qualification.

Implemented: independent Rust daemon and React/Tauri UI, private local IPC,
durable configuration with conflict handling, guided one/two-panel layouts,
clock/battery widgets, discovery/identification, location bindings, serial output
routing/recovery safeguards, manual brightness/sleep, temporary output previews,
and optional Claude/OpenCode context/activity producers.

Still outstanding for Linux v1: desktop redesign,
and the complete migration
and release-qualification workflows. These are software tickets, not checks that
can be closed merely by connecting hardware.

- [Roadmap #1](https://github.com/jschraub/fw16-ledmatrix/issues/1) and
  [PLAN.md](../PLAN.md): agreed scope and constraints.
- Feature specs:
  [daemon #2](https://github.com/jschraub/fw16-ledmatrix/issues/2),
  [devices #3](https://github.com/jschraub/fw16-ledmatrix/issues/3),
  [configuration #4](https://github.com/jschraub/fw16-ledmatrix/issues/4),
  [widgets #5](https://github.com/jschraub/fw16-ledmatrix/issues/5),
  [brightness/feedback #6](https://github.com/jschraub/fw16-ledmatrix/issues/6),
  [integrations #7](https://github.com/jschraub/fw16-ledmatrix/issues/7),
  [delivery/qualification #8](https://github.com/jschraub/fw16-ledmatrix/issues/8).
- [WIDGETS.md](WIDGETS.md): actual renderer, brightness, assignment and recovery limits.
- [INTEGRATIONS.md](INTEGRATIONS.md): producer installation, paths, schema and privacy.
- [SUPPORT.md](SUPPORT.md): evidence versus unverified platform support.
- [DEVELOPMENT.md](DEVELOPMENT.md): toolchains, dependencies, local IPC and
  the isolated desktop smoke check.
- [`poc/`](../poc/): preserved Python reference, tests, installer and hardware tools.
  Keep it intact for reference.

## Validation and CI checkpoint

At `3620592`: Rust tests, 25 desktop tests, workspace/all-target Clippy,
generated TypeScript bindings, debug no-bundle Tauri build, and isolated
lock-content check passed locally. [Build and test run 45253787176](https://github.com/jschraub/fw16-ledmatrix/actions/runs/45253787176)
was in progress at handoff inspection; check its final Linux/Windows result.
The numbers and baseline below describe the **earlier** session.

The baseline [run 45141501943](https://github.com/jschraub/fw16-ledmatrix/actions/runs/45141501943)
for `dc59d07` failed Clippy on Linux and Windows. This session fixed
`clippy::nonminimal_bool` in `matrix-claude` using `is_none_or`, without suppressing
the lint. The lifecycle test also now explicitly saves a clock-only layout,
instead of assuming the host has no battery.

Local validation passed **65 Rust tests**, **13 OpenCode producer tests**,
**23 UI tests**, formatting, workspace/all-target Clippy, generated-contract
comparison, frontend build and native Tauri debug no-bundle build. Rust was
**1.94.0**, Node **24.15.0**, npm **11.16.0**. These are separate from physical
observations in SUPPORT.md. Check the new pushed revision's Linux/Windows CI;
the failed baseline does not describe the fixed code, and local success is not
proof of remote CI success.

The machine's system Rust/Node are newer than the pins. This session used an
isolated rustup under `/tmp/opencode`:

```sh
env PATH="/tmp/opencode/cargo/bin:$PATH" RUSTUP_HOME=/tmp/opencode/rustup cargo test --locked
npx --yes --package=node@24.15.0 --package=npm@11.16.0 --call 'npm test'
```

Run the npm command in the relevant package directory. Temporary toolchains may
be cleared; reinstall the pinned prerequisites when needed. OpenCode integration
dependency installation reported 24 moderate vulnerabilities; no dependency
upgrade was attempted.

## Fresh-machine checkout and first launch

Use this section for a fresh environment. On the current laptop, begin with
the resume checkpoint above so existing bindings and USB ownership are understood.

1. Clone/pull `main`, inspect `git status`, and note the exact commit:

   ```sh
   git clone https://github.com/jschraub/fw16-ledmatrix.git
   cd fw16-ledmatrix
   git status --short --branch
   git rev-parse HEAD
   ```

   For an existing checkout, preserve local work before updating. Use
   `git pull --ff-only` only once the working tree and branch are understood.

2. Inventory and back up the existing POC setup: service definition, configuration,
   assistant settings, and the command used to start it. **Stop its service and
   any manually started POC process before Rust identification/output.** The
   historical user service is `matrixd.service`; verify the installed name before
   issuing stop/disable commands. Do not uninstall it or rewrite its settings as
   part of this first smoke test.

3. Install Rust 1.94.0, Node 24.15.0, and native Tauri dependencies from
   [DEVELOPMENT.md](DEVELOPMENT.md). Linux needs a real graphical user session.
   Windows needs MSVC/Windows SDK and WebView2. Run as the normal user, not root or
   Administrator. Review USB permissions/drivers instead of elevating the daemon.
   The POC's narrow VID/PID udev rule is reference material at
   `poc/udev/60-framework-ledmatrix.rules`; installation work belongs to #28.

4. Use separate configuration, IPC and feeds for qualification. Do not reuse
   production configuration with saved physical bindings on the first run.
   In **both** the daemon and desktop terminals, set the same values:

   **Linux / Bash**

   ```sh
   umask 077
   export HANDOFF_ROOT="$HOME/.local/share/fw16-matrix-handoff"
   mkdir -p "$HANDOFF_ROOT"
   export MATRIX_CONFIG="$HANDOFF_ROOT/config.json"
   export MATRIX_ENDPOINT="$HANDOFF_ROOT/ipc/daemon.sock"
   export MATRIX_FEEDS="$HANDOFF_ROOT/feeds"
   ```

   **Post-v1 Windows development / PowerShell**

   ```powershell
   $root = Join-Path $env:LOCALAPPDATA "fw16-matrix-handoff"
   New-Item -ItemType Directory -Force $root | Out-Null
   $env:MATRIX_CONFIG = Join-Path $root "config.json"
   $env:MATRIX_ENDPOINT = '\\.\pipe\fw16-matrix-handoff-' + $env:USERNAME
   $env:MATRIX_FEEDS = Join-Path $root "feeds"
   ```

   Use a new private directory. Unix unsafe existing directories are rejected,
   not repaired; on Windows review inherited per-user ACLs. Endpoint isolation
   does **not** isolate USB ownership: never run two daemons against the panels.
   Assistant producers must inherit the same `MATRIX_FEEDS` when activated later.

5. At repository root, capture the prerequisite probe:

   ```sh
   cargo run --locked -p matrix-daemon -- --probe
   ```

   It enumerates/probes prerequisites, not firmware or optical correctness.
   Save a reviewed/redacted copy with OS, desktop, display, driver and panel count.
   Then start the daemon in that terminal:

   ```sh
   cargo run --locked -p matrix-daemon
   ```

6. In the second terminal with the same environment, start the native UI:

   ```sh
   cd apps/desktop
   npm ci
   npm run tauri -- dev
   ```

   `npm run dev` alone is a browser-only frontend and cannot reach the Tauri bridge.
   The daemon's startup log still says “simulated”; that string does not establish
   whether assigned physical output is active. Inspect Devices and bindings.

## Fresh-machine hardware sequence

This is a general sequence, not the current session's completion checklist.
Consult SUPPORT.md for checks already exercised and remaining acceptance gaps.
On this laptop, physical module-removal checks remain paused as described above.

- [ ] Begin with **one panel**. In Devices, discover it and record port/location.
- [ ] Explicitly identify it. Check firmware, outline orientation and timeout.
  Cancel identification and close the UI during another identification.
  An unassigned panel restores blank/sleep, not an arbitrary logical role.
- [ ] Load configuration, refresh assignment candidates, visually identify the
  intended bay, bind the logical panel to that unique location, then Apply.
  Do not copy bindings from the other machine.
- [ ] Start with a sparse clock and conservative brightness. Confirm direction,
  clipping, missing-source blanks and actual readability. An on-screen preview
  is not a brightness calibration.
- [ ] Close/reopen the desktop and restart the daemon. Confirm saved layout,
  brightness and bay assignment persist, and closing the UI does not stop output.
- [ ] Add the second panel. Verify distinct roles before reconnect/suspend tests.
- [ ] Stop with Ctrl-C and inspect sleep/port release. Stop Rust before returning
  to the POC. Do not use output-queue flushing or firmware updates to mask faults.

**Windows blocker:** discovery currently returns unknown physical locations.
Assignment is deliberately disabled for unknown/ambiguous locations. Implement
the post-v1 native PnP location adapter in #42 using actual device properties. COM order
and duplicate USB serial numbers are not substitutes for bay identity.

**Recovery limit:** a write failure quarantines the connection because the
firmware parser can be inside a partial payload. The current retry path requires
the module to be absent for more than one two-second discovery interval; that
implementation requirement is not a supported physical-removal procedure for
this laptop. Keep removal paused until the input-loss blocker is addressed.
Rapid resume/re-enumeration is not proven safe. Open failures retry no sooner
than five seconds; static frames receive a 25-second keepalive. Investigate
deviations under #36.

## Hardware and interactive qualification tickets

These are open native sub-issues of the relevant feature specs. Add evidence and
code fixes to the existing ticket instead of creating duplicate “verify hardware”
tickets.

| Order | Ticket | Work to complete |
| --- | --- | --- |
| 1 | [#31 Capability evidence](https://github.com/jschraub/fw16-ledmatrix/issues/31) | Probe the actual laptop and interactive OS; publish the tested environment and prerequisite gaps. |
| 2 | [#34 Discovery and identification](https://github.com/jschraub/fw16-ledmatrix/issues/34) | Driver/access/exclusivity, actual firmware, port/wake settle timing, outline, cancel/disconnect and restoration. |
| 3 | [#35 Persistent assignment](https://github.com/jschraub/fw16-ledmatrix/issues/35) | Verify Linux sysfs bay identity, restart/reorder safety, one/two-panel routing and ambiguity rejection. |
| 4 | [#32 Layout/clock optics](https://github.com/jschraub/fw16-ledmatrix/issues/32) | Compare all three templates with POC and Rust frames; sparse digits, midnight, leading zero, independent intensity and overflow `XX`. |
| 5 | [#37 Brightness/sleep](https://github.com/jschraub/fw16-ledmatrix/issues/37) | Calibrate each module; verify inheritance/overrides, min/max, explicit sleep, wake and identification restoration. |
| 6 | [#33 Battery/power](https://github.com/jschraub/fw16-ledmatrix/issues/33) | Compare OS charge/AC/charging state; thresholds, missing/error/recovery, repeated widgets and resume. |
| 7 | [#38 Temporary previews](https://github.com/jschraub/fw16-ledmatrix/issues/38) | Expiry/cancel/UI death/unplug; concurrent Apply restores latest saved state; second owner is busy; identification priority and restoration latency. |
| 8 | [#39 OpenCode activation](https://github.com/jschraub/fw16-ledmatrix/issues/39) | Explicit V2 installation/reload, paths/ACLs, main/OpenAI filtering, model overrides, busy/idle, multiple instances and crash expiry. |
| 9 | [#40 Claude activation](https://github.com/jschraub/fw16-ledmatrix/issues/40) | Wrap the existing status line unchanged; validate shell quoting, hook edges, 15-second refresh, expiry and Linux permissions. |
| 10 | [#36 Recovery/keepalive](https://github.com/jschraub/fw16-ledmatrix/issues/36) | Unplug/resume, fast re-enumeration, partial writes, competing owners, shutdown and the focused full-day static-content soak. |

These checks can reveal unfinished adapter work, not only test failures. Record
unavailable OSes as pending. One Linux laptop does not qualify Windows, and a
Windows runner does not qualify physical Windows hardware.

## Remaining software tickets and delivery order

| Ticket | Outcome and prerequisites |
| --- | --- |
| [#41 Desktop redesign](https://github.com/jschraub/fw16-ledmatrix/issues/41) | Iterate on the selected panel map, demonstrate everyday workflows and obtain owner sign-off before #30. |
| [#30 Release qualification](https://github.com/jschraub/fw16-ledmatrix/issues/30) | Dated supported-environment evidence, POC parity, disruptive checks by approval, full-day product soak and green CI. Depends on #29. The device-focused #36 soak alone does not complete release qualification. |

Prioritize #41 and remaining Linux qualification evidence for #30.
Keep each ticket's implementation,
tests and commit separate. Use native GitHub parent/dependency relationships for
new follow-ups, then verify them. Do not close parent specs while required
qualification or software work remains.

## Code entry points

| Area | Files |
| --- | --- |
| USB discovery, stable identity, serial lifecycle, priorities | `crates/matrix-daemon/src/devices.rs`, `firmware.rs`, `crates/matrix-core/src/protocol.rs` |
| Pure geometry, widgets and brightness | `crates/matrix-core/src/layout.rs`, `brightness.rs`, `config.rs` |
| Persistence, revisions, preview leases | `crates/matrix-daemon/src/config.rs` |
| Runtime/source supervision and capability report | `crates/matrix-daemon/src/main.rs`, `power.rs`, `feeds.rs`, `probe.rs` |
| IPC, paths, contracts | `crates/matrix-client/src/`, especially `lib.rs` and `local/` |
| Native bridge and desktop editors | `apps/desktop/src-tauri/src/main.rs`, `apps/desktop/src/` |
| Claude/OpenCode producers | `crates/matrix-claude/`, `integrations/opencode/` |
| Tests and build matrix | `crates/*/tests/`, `apps/desktop/src/*.test.tsx`, `.github/workflows/ci.yml` |

React paints Rust frames. Keep rendering/validation/persistence/hardware in Rust.
Retain local-only IPC, independent UI/daemon lifetimes and guided layouts. Do not
expand into a generic display framework or silently change assistant settings.
The clock remains 24-hour until a legible AM/PM design is agreed.

## Validation and evidence to bring back

At repository root:

```sh
cargo fmt --all -- --check
cargo test --locked
cargo run --locked -p matrix-client --example export-types
git diff -- apps/desktop/src/bindings
```

In `integrations/opencode`: `npm ci --ignore-scripts`, then `npm test`.
In `apps/desktop`: `npm ci`, `npm test`, `npm run build`, then
`npm run tauri -- build --debug --no-bundle`.
After frontend assets exist, run at root:

```sh
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Node is required by the Rust integration fixtures too. Regenerate bindings after
contract changes; do not hand-edit them. A no-bundle native build is not an
installer or proof of window interaction.

For each manual result, comment on the linked ticket with:

```text
Date / commit:
Ticket and exact scenario:
OS/build, desktop/session type, display(s):
Panel count, bay/location, driver, firmware:
Expected / observed behavior:
Pass / fail / pending, reproduction steps:
Reviewed logs, frame/photo evidence, timing or CPU/memory measurements:
Fix commit and validation:
Remaining environments or blockers:
```

Review every attachment for tokens, conversation text, usernames and local paths.
Never attach credential files or raw assistant payloads. Update SUPPORT.md with
verified environments and WIDGETS.md with any corrected hardware limits.
Use dated measurements for keepalive, idle CPU/memory and restoration latency.
Finish with a clean commit per ticket, push it, attach evidence, and close only
the acceptance criteria actually completed.

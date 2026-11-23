# Hardware-session handoff

Updated **2026-11-23** after the attended #26 Linux lifecycle checks.
Repository: [jschraub/fw16-ledmatrix](https://github.com/jschraub/fw16-ledmatrix).

**#26 is closed for Linux v1. Next: #27 feedback overlays.** Windows
development (#42) and support qualification (#43) are post-v1. Linux optical
lock, display-off and an attended system suspend/resume cycle passed. Details are in
[SUPPORT.md](SUPPORT.md#initial-cachyos-hardware-session--2026-11-23).
Physical module-removal testing is paused because it interrupted keyboard and
touchpad input. Full-day soak and Windows bay qualification remain pending.

## Resume this machine

- At last inspection the rebuilt `fw16-matrix-handoff-daemon.service` was active
  with both ports open after resume. `fw16-matrix-panel-map-desktop.service`
  was relaunched with the `f912da7` desktop and its native window verified open.
  The new display-power and curve controls are under **Configuration → Brightness**.
  These are transient **user** units. Closing the window leaves the daemon running.
- POC `matrixd.service`: stopped, **still enabled**. It may start on a later login.
  Check actual services and serial ownership before launching another daemon.
- Private POC service/settings/integration backup:
  `$HOME/.local/share/fw16-matrix-handoff/poc-backup/`. Keep it private; it is not
  a diagnostic attachment. Assistant settings were preserved, not migrated.
- Isolated configuration: **revision 8**, right clock with empty gauge slot,
  left full battery gauge, clock intensity 180, global manual brightness 32,
  sleep disabled, no per-panel overrides, no saved follow-display or lock preset.
  Use Load saved configuration or Cancel
  draft to discard any unsaved preview draft instead of applying it.

Inspect the runtime state first:

```sh
git status --short --branch
git rev-parse HEAD
systemctl --user show matrixd.service fw16-matrix-handoff-daemon.service fw16-matrix-panel-map-desktop.service fw16-matrix-handoff-desktop.service --property=Id,ActiveState,MainPID
fuser -v /dev/ttyACM{0,1}
```

Both Rust processes use the same isolated environment:

```sh
export HANDOFF_ROOT="$HOME/.local/share/fw16-matrix-handoff"
export MATRIX_CONFIG="$HANDOFF_ROOT/config.json"
export MATRIX_ENDPOINT="$HANDOFF_ROOT/ipc/daemon.sock"
export MATRIX_FEEDS="$HANDOFF_ROOT/feeds"
```

Transient units are not installed login startup; inspect again after logout or
reboot. If they are gone, first confirm the POC is stopped, then use the normal
launch commands below with this environment. The saved configuration already
has physical assignments, so restarting this daemon can immediately drive both
panels. To stop the qualification processes explicitly:

```sh
systemctl --user stop fw16-matrix-panel-map-desktop.service fw16-matrix-handoff-daemon.service
```

Before restoring the POC, review its unit: the installed WorkingDirectory points
to the repository root, while preserved Python sources now live under `poc/`.
Rollback was not exercised; preserve settings and verify a single USB writer.

## Immediate test and session results

1. Recheck the units and ownership above. If the daemon is still active, only
   relaunch the desktop (preserve any draft); use `target/debug/matrix-desktop`
   with the isolated environment. No persistent session policies are enabled.
2. Continue Linux development with #27, or the independent #22 lane. Windows
   PnP identity and display/session qualification moved to #42/#43. Physical
   module removal on this laptop remains paused.

| Area | Evidence from this session |
| --- | --- |
| #31 capability screen | Native two-panel report matched CLI; zero-panel report and unreachable-daemon error passed using isolated instances. |
| #32/#37 optics | Left numeric layout showed legible overflow `XX` at 100% battery and correct gauge placement. Right sparse clock at brightness 3/intensity 180 was dim but readable. Both preview checks restored saved output. |
| #38 preview ownership/restoration | Competing client rejected; owner EOF released lease. Isolated expiry/cancel/owner-kill restored concurrent Apply. On hardware the left clock appeared after expiry, rollback restored its battery gauge, and the right clock stayed visible. The 1.99 s acknowledgment includes human reaction; it is not LED latency. |
| #26 display/session | Right clock visibly followed screen 55% → 25%; left override held. Owner confirmed per-panel locked roles, DPMS sleep/wake, and both roles and input after system resume. Linear/quadratic mapping, manual fallback, monitor restart and latest-output restoration tested in fixtures. Windows remains unqualified. |
| #41 product design | Owner chose Option A, the left/right panel map (`1bad83c`). Select a panel to edit widgets/brightness; diagnostics have their own destination. Current UI accepted **for now**, not final sign-off. [Prototype branch](https://github.com/jschraub/fw16-ledmatrix/tree/prototype/41-panel-map); #41 still blocks release #30. |

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

Still outstanding: account-bound quota sources, remaining display/session policies,
feedback overlays, production packaging/login startup, and the complete migration
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
- [DEVELOPMENT.md](DEVELOPMENT.md): toolchains, dependencies and local IPC.
  Its introductory #9-only description is historical: the current daemon **does**
  open serial devices after identification or physical assignment.
- [`poc/`](../poc/): preserved Python reference, tests, installer and hardware tools.
  Keep it intact for comparison and rollback.

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
| [#22 Account-bound usage windows](https://github.com/jschraub/fw16-ledmatrix/issues/22) | Read-only supported credential discovery, independent Claude/OpenAI windows, correct durations, account changes, caching/expiry, 401/schema failures and redaction. Depends on closed #20/#21. Live requests require explicit opt-in; never paste tokens into config or take ownership of refresh. |
| [#27 Feedback overlays](https://github.com/jschraub/fw16-ledmatrix/issues/27) | Enabled/target/duration settings, latest-event replacement, ambient updates underneath, supervised real audio state and bounded charging indication. Depends on #26 and closed #18. Preserve identify/preview priority and complete serial commands. |
| [#28 Packaging/startup](https://github.com/jschraub/fw16-ledmatrix/issues/28) | Choose installers/startup from platform evidence, install outside the checkout, narrow USB access, login startup, upgrade/uninstall and signing limits. Depends on closed #11/#14/#24. Do not confuse existing explicit desktop “Start daemon” with login startup. |
| [#29 Migration/diagnostics](https://github.com/jschraub/fw16-ledmatrix/issues/29) | Explicit POC stop/disable, preserved settings, rollback without dual writers, keyboard workflows and redacted diagnostics. Depends on #22/#27/#28 and closed #15/#17. |
| [#41 Desktop redesign](https://github.com/jschraub/fw16-ledmatrix/issues/41) | Iterate on the selected panel map, demonstrate everyday workflows and obtain owner sign-off before #30. |
| [#30 Release qualification](https://github.com/jschraub/fw16-ledmatrix/issues/30) | Dated supported-environment evidence, POC parity, disruptive checks by approval, full-day product soak and green CI. Depends on #29. The device-focused #36 soak alone does not complete release qualification. |

Suggested development lanes: #22 independently; #27 now that #26 is closed; #28 after initial
platform evidence. Converge on #29, then #30. Keep each ticket's implementation,
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

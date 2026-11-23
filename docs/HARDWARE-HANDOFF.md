# Handoff to a panel-equipped machine

Status checked **2026-11-23**, against `main` at `edbeb99`.
Repository: [jschraub/fw16-ledmatrix](https://github.com/jschraub/fw16-ledmatrix).

Start with the CI blocker below, then discovery and identification on one panel.
Do not start with the full-day soak or assume Windows bay assignment works.

## Current state and authoritative references

Software slices #9–#21 and #23–#25 are closed with individual commits and
validation records. Their closure records software delivery, **not physical
qualification**. No panels were connected on the development machine, and native
window interaction was unavailable there.

Implemented: independent Rust daemon and React/Tauri UI, private local IPC,
durable configuration with conflict handling, guided one/two-panel layouts,
clock/battery widgets, discovery/identification, location bindings, serial output
routing/recovery safeguards, manual brightness/sleep, temporary output previews,
and optional Claude/OpenCode context/activity producers.

Still unimplemented: account-bound quota sources, display/session policies,
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

## First blocker: CI is red

The previous handoff said the rerun was pending. It has completed:
[run 40831022539](https://github.com/jschraub/fw16-ledmatrix/actions/runs/40831022539)
failed Clippy on **both Linux and Windows**.

The recorded diagnostic is `clippy::nonminimal_bool` in
`crates/matrix-claude/src/main.rs:15`:

```rust
if !hook && !args.first().is_some_and(|arg| arg == "statusline") {
```

Clippy recommends `args.first().is_none_or(|arg| arg != "statusline")` for the
second expression. Fix and validate with the pinned Rust **1.94.0** toolchain,
then check a new Linux/Windows CI run. Do not suppress the lint or describe the
current branch as green. This document does not change application code.

Previous local validation passed 59 Rust tests, 13 OpenCode producer tests,
23 UI tests and the frontend build. The earlier local Clippy success does not
override the recorded CI failure. Re-run validation on the destination machine.

## Safe checkout and first launch

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

   **Windows / PowerShell**

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

## First hardware session

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
the native PnP location adapter in #35 using actual device properties. COM order
and duplicate USB serial numbers are not substitutes for bay identity.

**Recovery limit:** a write failure quarantines the connection because the
firmware parser can be inside a partial payload. Disconnect the module for more
than one two-second discovery interval before retrying. Rapid resume/re-enumeration
is not proven safe. Open failures retry no sooner than five seconds; static
frames receive a 25-second keepalive. Investigate deviations under #36.

## Hardware and interactive qualification tickets

These are open native sub-issues of the relevant feature specs. Add evidence and
code fixes to the existing ticket instead of creating duplicate “verify hardware”
tickets.

| Order | Ticket | Work to complete |
| --- | --- | --- |
| 1 | [#31 Capability evidence](https://github.com/jschraub/fw16-ledmatrix/issues/31) | Probe the actual laptop and interactive OS; publish the tested environment and prerequisite gaps. |
| 2 | [#34 Discovery and identification](https://github.com/jschraub/fw16-ledmatrix/issues/34) | Driver/access/exclusivity, actual firmware, port/wake settle timing, outline, cancel/disconnect and restoration. |
| 3 | [#35 Persistent assignment](https://github.com/jschraub/fw16-ledmatrix/issues/35) | Finish Windows PnP location identity; verify Linux sysfs bay identity, restart/reorder safety, one/two-panel routing and ambiguity rejection. |
| 4 | [#32 Layout/clock optics](https://github.com/jschraub/fw16-ledmatrix/issues/32) | Compare all three templates with POC and Rust frames; sparse digits, midnight, leading zero, independent intensity and overflow `XX`. |
| 5 | [#37 Brightness/sleep](https://github.com/jschraub/fw16-ledmatrix/issues/37) | Calibrate each module; verify inheritance/overrides, min/max, explicit sleep, wake and identification restoration. |
| 6 | [#33 Battery/power](https://github.com/jschraub/fw16-ledmatrix/issues/33) | Compare OS charge/AC/charging state; thresholds, missing/error/recovery, repeated widgets and resume. |
| 7 | [#38 Temporary previews](https://github.com/jschraub/fw16-ledmatrix/issues/38) | Expiry/cancel/UI death/unplug; concurrent Apply restores latest saved state; second owner is busy; identification priority and restoration latency. |
| 8 | [#39 OpenCode activation](https://github.com/jschraub/fw16-ledmatrix/issues/39) | Explicit V2 installation/reload, paths/ACLs, main/OpenAI filtering, model overrides, busy/idle, multiple instances and crash expiry. |
| 9 | [#40 Claude activation](https://github.com/jschraub/fw16-ledmatrix/issues/40) | Wrap the existing status line unchanged; validate shell quoting, hook edges, 15-second refresh, expiry and Windows ACLs. |
| 10 | [#36 Recovery/keepalive](https://github.com/jschraub/fw16-ledmatrix/issues/36) | Unplug/resume, fast re-enumeration, partial writes, competing owners, shutdown and the focused full-day static-content soak. |

These checks can reveal unfinished adapter work, not only test failures. Record
unavailable OSes as pending. One Linux laptop does not qualify Windows, and a
Windows runner does not qualify physical Windows hardware.

## Remaining software tickets and delivery order

| Ticket | Outcome and prerequisites |
| --- | --- |
| [#22 Account-bound usage windows](https://github.com/jschraub/fw16-ledmatrix/issues/22) | Read-only supported credential discovery, independent Claude/OpenAI windows, correct durations, account changes, caching/expiry, 401/schema failures and redaction. Depends on closed #20/#21. Live requests require explicit opt-in; never paste tokens into config or take ownership of refresh. |
| [#26 Display/session policy](https://github.com/jschraub/fw16-ledmatrix/issues/26) | Selected-display brightness adapters, min/max/curves, manual fallback, distinct lock/display-off/suspend signals and latest-state resume. Depends on closed #11/#24/#25; actual desktop evidence is still needed. |
| [#27 Feedback overlays](https://github.com/jschraub/fw16-ledmatrix/issues/27) | Enabled/target/duration settings, latest-event replacement, ambient updates underneath, supervised real audio state and bounded charging indication. Depends on #26 and closed #18. Preserve identify/preview priority and complete serial commands. |
| [#28 Packaging/startup](https://github.com/jschraub/fw16-ledmatrix/issues/28) | Choose installers/startup from platform evidence, install outside the checkout, narrow USB access, login startup, upgrade/uninstall and signing limits. Depends on closed #11/#14/#24. Do not confuse existing explicit desktop “Start daemon” with login startup. |
| [#29 Migration/diagnostics](https://github.com/jschraub/fw16-ledmatrix/issues/29) | Explicit POC stop/disable, preserved settings, rollback without dual writers, keyboard workflows and redacted diagnostics. Depends on #22/#27/#28 and closed #15/#17. |
| [#30 Release qualification](https://github.com/jschraub/fw16-ledmatrix/issues/30) | Dated supported-environment evidence, POC parity, disruptive checks by approval, full-day product soak and green CI. Depends on #29. The device-focused #36 soak alone does not complete release qualification. |

Suggested development lanes: #22 independently; #26 then #27; #28 after initial
platform evidence. Converge on #29, then #30. Keep each ticket's implementation,
tests and commit separate. Use native GitHub parent/dependency relationships for
new follow-ups, then verify them. Do not close parent specs while required
qualification or software work remains.

## Code entry points

| Area | Files |
| --- | --- |
| USB discovery, stable identity, serial lifecycle, priorities | `crates/matrix-daemon/src/devices.rs`, `crates/matrix-core/src/protocol.rs` |
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

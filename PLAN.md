# Framework LED Matrix application — proposed implementation plan

Status: proposed for review; no rewrite implementation is included in this change.

## 1. Agreed direction

- Target the Framework Laptop 16 LED Matrix modules first, supporting one or two
  panels. Leave room for later hardware expansion without building a generic
  display platform now.
- Support Linux and Windows for both the background daemon and desktop UI.
- Use Rust for the daemon and shared rendering/configuration logic.
- Use Tauri with React and TypeScript for the desktop UI.
- Use guided layout templates with configurable widget slots, not a freeform
  drag-and-drop editor in the first release.
- Keep the daemon independent of the UI: closing the UI must not stop rendering.
- Preserve the Python application under `poc/` as a reference during migration.

The remainder of this document proposes details; it does not make the open
product decisions at the end final.

## 2. First-release scope

### Devices

- Discover supported modules and display connection/firmware health.
- Identify a selected panel with a temporary visible pattern, then assign it a
  logical role such as left or right. Restore its normal output afterward.
- Support one-panel installations and disconnected assigned panels.
- Reconnect after unplug, suspend/resume, or serial errors without losing config.
- Persist assignment using platform device-location information where reliable;
  ask for reassignment rather than silently swapping panels when identity is
  ambiguous. Both modules may report the same USB serial number.

### Brightness and display behavior

- Application defaults with explicit per-panel overrides.
- Manual brightness or follow-screen mode, with minimum/maximum limits and a
  simple configurable mapping curve.
- Explicit display selection when following screen brightness, plus a documented
  manual fallback when the selected display or capability is unavailable.
- Configurable behavior for display-off/lock events where supported, and safe
  suspend/resume handling. These are separate signals, not assumed equivalent.
- Distinguish intentional off/sleep from the minimum visible brightness.
- Separate panel-global brightness from per-widget relative greyscale intensity.
- Clamp pulses and overlays to the configured limits; identify potentially
  illegible content with calibration guidance rather than assuming one universal
  visibility threshold.

### Layouts and widgets

- Templates own fixed slot rectangles on the actual 9×34 pixel surface.
- Widget types declare supported dimensions and presentation variants. The daemon
  validates compatibility, bounds, padding, and intensity ranges.
- Allow empty slots, multiple instances of a widget, and independent per-panel
  template selection. Missing data does not cause layout reflow.
- Initial templates: clock + gauge; number + two gauges; full-panel gauge.
  Preserve today's machine and assistant layouts as default presets. Final
  slot geometry and compatible widget variants must be pinned by render tests.
- Initial widgets: clock, battery, assistant context, assistant activity, and
  assistant usage windows. Claude and OpenCode/OpenAI are source choices rather
  than global, mutually exclusive operating modes.
- Configure presentation where meaningful: clock format, gauge direction,
  thresholds, relative intensity, source/account/session selection, and activity
  emphasis. Expose only options supported by the selected slot/variant.
- Define overflow honestly (for example, `XX` for a number that cannot fit), and
  expose unavailable/stale data as status rather than a plausible zero.
- Optional temporary volume, brightness, and AC-change overlays, with panel
  target, duration, and enablement controls. New state replaces old state rather
  than building a queue of obsolete notifications.
- Preserve charging indication as an optional bounded effect. Global effects
  affect the whole panel and must be labeled that way in the UI.

### Desktop experience

1. **Devices:** discovery, identification, assignment, capability status.
2. **Display:** brightness policy and sleep/overlay settings.
3. **Layouts:** template selection, slot assignments, widget settings.
4. **Integrations:** setup instructions, source/session status, last update/errors.
5. **Diagnostics:** daemon version/health, reconnect state, redacted logs.

Show a live pixel preview sourced from the Rust renderer. Support simulated
panels and sample data for development and configuration without hardware; label
simulation clearly so it cannot be mistaken for live telemetry.

### Not in the first release

- Other display hardware, remote/network administration, or multi-user control.
- A plugin marketplace, dynamically loaded native plugins, arbitrary scripts
  executed by the daemon, or a universal widget scripting engine.
- A freeform layout editor, cloud sync, or automatic profile switching.
- OAuth sign-in/token refresh owned by this application.
- Firmware updates or a claim of high-frame-rate greyscale animation.

## 3. Architecture and ownership

```text
React/TypeScript UI
        │ narrow Tauri commands and event forwarding
Tauri Rust client bridge
        │ versioned local control interface
Rust daemon (one per logged-in user)
        ├── configuration validation, revisions, atomic persistence
        ├── sources and freshness/session selection
        ├── template composition and pure widget rendering
        ├── brightness, sleep, overlay arbitration
        ├── per-panel serial scheduling and recovery
        └── Linux / Windows platform adapters
```

Only the daemon opens hardware ports, reads provider credentials, polls sources,
and commits configuration. The Tauri process is a client, not the daemon's host.
It may explicitly request daemon startup, but closing it never stops the daemon.

### Core modules

| Module | Owns | Keeps private |
| --- | --- | --- |
| Configuration | Versioned desired state, validation, effective settings | Defaults, migrations, persistence mechanics |
| Rendering | Templates + widget settings + normalized readings → pixel frames | Fonts, clipping, geometry, intensity calculations |
| Sources | Typed readings with freshness and health | Polling, hooks, credential access, platform calls |
| Display runtime | Current desired output and panel lifecycle | Timers, effect arbitration, frame coalescing, keepalives |
| Framework device | Discovery, assignment information, protocol I/O | Platform serial handles, encoding, wake/reconnect sequencing |
| Local control | Configuration commands and observable runtime state | Framing, reconnect, transport-specific details |

Sources and widgets are separate: one source can feed several widgets, and
multiple instances must not duplicate provider polling. Readings carry
timestamps and availability; stale cached values never silently become fresh.
Isolate failures by source and panel so a broken quota endpoint cannot freeze
the clock or another panel.

Keep rendering and configuration free of OS I/O. Inject time and sample readings
for deterministic tests. Start with a small number of modules, not a trait and
crate for every widget. Use real seams for the Linux/Windows adapters and the
hardware/simulated output paths.

### Proposed repository shape

```text
poc/                    preserved Python implementation and historical docs
crates/
  matrix-core/          config model, templates, widgets, pure rendering
  matrix-daemon/        runtime, sources, platform/device adapters, local control
  matrix-client/        shared Rust local-control types and client
apps/
  desktop/
    src/                React + TypeScript
    src-tauri/          Tauri shell and local-control bridge
docs/                   implementation notes and support documentation
```

This is a future layout, not empty scaffolding to create before review. The root
Cargo workspace can include the desktop Rust crate. Prefer a conventional React
build using Vite; use a committed package-manager lockfile and pinned toolchains.

Likely dependencies include Tokio, Serde, tracing, and a cross-platform serial
library. Choose exact libraries after the platform spike; serial enumeration
alone must not be mistaken for reliable device-location identity.

## 4. Local control and configuration lifecycle

Proposed transport: a user-restricted Unix-domain socket on Linux and named pipe
on Windows, hidden behind the shared Rust client. Use bounded, length-framed JSON
messages initially. No TCP listener, browser-accessible server, or remote mode.
Validate same-user access/permissions, message limits, and peer handling during
the first cross-platform spike.

The interface needs to support:

- Negotiate protocol version and report daemon capabilities.
- Read saved configuration and its revision.
- Validate a draft and obtain structured field/slot errors.
- Render a draft preview without changing hardware or saved configuration.
- Optionally preview a validated draft on hardware using a short-lived lease.
- Apply a draft against an expected revision and atomically persist it.
- Subscribe to health, normalized readings, and coalesced preview frames.
- Identify a panel temporarily and cancel/restore that output.

Use one active hardware-preview lease per daemon initially; other clients can
still inspect and render drafts. On cancellation, client disconnect, or lease
expiry, restore the latest applied state. A bounded expiry handles client crashes.
Reject conflicting applies rather than silently overwriting another client's
changes. Apply/cancel and identification priority must have deterministic tests.

Persist a versioned JSON configuration in the user's platform config directory.
Store stable logical panel assignments, selected templates, widget instances, and
policy settings—not credentials, serial handles, or transient readings. Derive
TypeScript interface types from the Rust definitions where practical; runtime
validation stays authoritative in Rust.

Validate a draft before persistence, use atomic platform-appropriate replacement,
and publish the new applied revision only after a successful save. Device failure
does not roll back valid desired configuration: retain it, report degraded output,
and retry. A corrupt file at startup must be preserved for recovery, not silently
overwritten with defaults. Define explicit schema migrations before changing a
released format.

## 5. Cross-platform strategy

| Concern | Linux | Windows |
| --- | --- | --- |
| Startup | systemd user unit on supported desktops | Per-user login startup mechanism, selected during packaging |
| Device access | Narrow udev rule and active-user access | Supported CDC serial driver and per-user port access, verified on hardware |
| Device identity | USB topology/location, not serial alone | Enumerate device location through Windows device APIs; verify stability |
| Power/audio | Existing sysfs/udev and audio behavior as reference | Windows power notifications and Core Audio adapters |
| Screen brightness | Detect supported backlight/display access | Detect supported internal display brightness interfaces |
| Lifecycle | Desktop-session events where available | User-session power/display/lock notifications |
| Integration storage | Platform config/runtime paths | User-specific config/runtime storage with explicit expiry |

Do not promise identical capability coverage for every desktop, display, or
driver. Report supported/unsupported/error states with actionable fallback
behavior. Scope supported Linux distributions/desktops and Windows versions in
the platform spike and publish the tested matrix before release.

The daemon is a background user-session process, not a privileged machine-wide
Windows Service. Routine rendering requires no root/admin access. Setup may need
elevation for narrow OS installation steps only, explained to the user.

Keep one daemon instance per user and exclusive serial ownership. Detect a
competing daemon or the POC and provide a useful error; do not repeatedly steal
the device. Multi-user active-session arbitration is out of scope for v1.

### Integration migration

Preserve the useful metadata-only contract of the current integrations, but do
not assume Bash, XDG paths, or logout-cleared tmpfs exist on Windows. Specify a
versioned snapshot format with timestamps/heartbeat expiry and bounded payloads.
Use atomic per-user files for producer independence; validate IDs and paths.

Provide a small cross-platform Rust helper for Claude hooks/status-line capture,
including transparent forwarding to an existing status-line command. Update the
OpenCode JavaScript producer for platform paths and the new snapshot contract.
Do not edit client settings automatically; offer instructions and explicit setup
actions. Verify Windows command quoting and actual client credential locations.
Do not assume Linux credential discovery works unchanged on Windows.

Keep external-tool credentials read-only and out of the UI, configuration,
snapshots, and logs. Unsupported credential storage should produce an explicit
integration status, not a request to copy tokens into settings. Internal quota
endpoints remain optional and failure-tolerant; endpoint changes must not break
session activity/context or unrelated widgets.

## 6. Hardware constraints to preserve

The measurements and their caveats remain in [poc/README.md](poc/README.md) and
[poc/DECISIONS.md](poc/DECISIONS.md). Treat them as reference observations to
recheck on supported hardware/OS combinations, not universal guarantees.

- 9×34 greyscale panels; global brightness multiplies pixel intensity.
- Duplicate serial numbers make serial-only panel assignment unsafe.
- Hold ports open; wake settling and keepalive are part of the device lifecycle.
- Greyscale frames were measured around 169 ms versus about 25 ms for 1-bit
  frames. Do not design the UI around smooth greyscale animation.
- Serialize writes per panel. Coalesce obsolete pending frames with bounded
  queues; never interrupt a partially transmitted command to prioritize an
  overlay. Independent panels must not block each other.
- Restore brightness and latest content on reconnect, and sleep on clean exit
  where possible. A crash cannot guarantee a clean hardware shutdown.
- Calibrate with representative sparse content, not full-panel fills. The POC's
  visibility floor is measured evidence for its machine, not a universal default
  that makes every future widget legible at any intensity.

## 7. Implementation milestones and review gates

### Milestone 0 — preserve and agree on scope

- Move the existing application intact into `poc/` and update root navigation.
- Run its Python and JavaScript suites from `poc/`.
- Review this plan before adding the Rust/Tauri implementation.

Exit: working reference preserved; architecture and open scope decisions reviewed.

### Milestone 1 — portability and lifecycle spike

- Establish a minimal Rust workspace with simulated output and a device probe.
- On Linux and Windows, enumerate/identify both panels, send representative
  frames, and test reconnect plus stable assignment across suspend/resume.
- Exercise same-user local control, user-session startup, brightness/power/audio
  capability discovery, and credential/snapshot path discovery on both OSes.
- Select dependencies and record supported environments and measured limitations.

Exit: demonstrated Windows/Linux feasibility, or explicit reviewed limitations.
Do this before investing in a polished UI or claiming platform parity.

### Milestone 2 — headless Rust parity

- Implement pure rendering and wire encoding against representative POC fixtures.
- Reproduce current clock/battery and assistant presets, brightness mapping,
  charging indication, overlays, and source freshness behavior.
- Add serial scheduling, keepalives, reconnect, single-instance handling, and
  independent source supervision.
- Port integrations with cross-platform helpers and validate on both OSes.

Exit: the Rust daemon can run the current experience without a UI; golden frames,
source fixtures, and lifecycle tests pass. POC remains available for comparison.

### Milestone 3 — configurable headless product

- Add templates, widget instances, capability reporting, and effective policies.
- Implement revisioned configuration persistence and the local control interface.
- Implement draft rendering, subscriptions, temporary hardware preview, and
  identification leases with restoration.

Exit: a test client can configure a running daemon without restarting it; invalid
drafts, stale revisions, disconnected clients, and failed saves behave predictably.

### Milestone 4 — React/Tauri configuration UI

- Implement devices, display policy, guided layout editing, integrations, and
  diagnostics screens with keyboard-accessible controls.
- Render the Rust-produced preview frames; support explicit sample-data mode.
- Handle daemon missing/startup/reconnect, incompatible versions, failed saves,
  and preview/apply/cancel without losing the user's draft.

Exit: a user can install/connect, assign panels, configure widgets/brightness,
preview changes, and save them without editing files or using a terminal.

### Milestone 5 — packaging and release qualification

- Build signed/reproducibly versioned artifacts where feasible and document
  signing/distribution requirements. Exact installer formats follow the support
  matrix and packaging spike; do not add an auto-updater in v1 by default.
- Install/update/uninstall both binaries and per-user startup independently of
  the source checkout. Explain settings retention and optional cleanup.
- Provide an explicit POC migration path: stop/disable its service before starting
  Rust, preserve external client settings, and document rollback without running
  both against the same ports. Do not treat this plan's file move as migration.
- Complete a full-day soak and the hardware/platform qualification matrix.

Exit: both supported OS targets pass the documented release checks; outstanding
capability gaps and integration limitations are visible to users.

## 8. Validation strategy

- **Core:** golden pixel frames/encoded commands, template compatibility, bounds,
  brightness invariants, effects, overflow, missing/stale data, deterministic time.
- **Runtime:** fake clock/transport, disconnects, partial writes, backpressure,
  keepalive deadlines, latest-frame coalescing, source failures, shutdown.
- **Configuration/control:** round trips, invalid/corrupt files, atomic-save
  failure, revisions, malformed/oversized messages, access control, protocol
  mismatch, multi-client conflicts, preview expiry and restoration.
- **Integrations:** metadata-only fixtures, account changes, token expiry,
  endpoint/schema failures, heartbeat expiry, main-session selection, path safety,
  producer-to-consumer tests on Linux and Windows.
- **UI:** guided-slot validation feedback, unsaved drafts, Apply/Cancel,
  daemon reconnect/version errors, unavailable capabilities, accessible controls.
- **CI:** Rust formatting/lint/tests and desktop/type/build checks on Linux and
  Windows. Separate simulated coverage from real-hardware coverage.
- **Hardware:** identify each bay, validate orientation and sparse-content floor,
  suspend/resume, device loss, display off/lock, audio restart, screen tracking,
  provider expiry/recovery, single-panel use, and a full-day idle/static-content
  soak. Record tested OS/device/firmware combinations.

No hardware, provider credentials, or elevated commands should be required for
the standard automated suite. Run disruptive hardware tests only explicitly.

## 9. Decisions still needed

These need not block preserving the POC or reviewing the architecture:

1. **Supported environments:** proposed Windows 11 and a small named Linux
   desktop/distribution matrix; verify actual hardware and desktop access first.
2. **Clock options:** offer 24-hour and 12-hour variants, but decide on an honest
   AM/PM treatment that fits before promising parity between them.
3. **Session selection:** proposed latest active main session by default, with
   explicit pinning when multiple sessions are available; define expired-pin UX.
4. **Layout customization:** v1 offers fixed templates and slot settings only;
   advanced position/size editing is deferred unless a concrete use case warrants
   the additional validation/UI complexity.
5. **Profiles:** proposed one active saved configuration in v1; named profiles can
   follow without changing the separation of settings, layout, and runtime state.
6. **Distribution:** choose Linux package formats, Windows installer/startup
   mechanism, signing, and supported upgrades after the platform spike.

## 10. Risks and mitigations

- **Rewrite regressions:** retain the POC, port representative fixtures, compare
  rendered output, and require hardware qualification before switching defaults.
- **Windows identity/system integration:** spike early; report limitations rather
  than hiding unsupported behavior behind nonfunctional toggles.
- **Undocumented provider endpoints/auth layouts:** keep adapters isolated,
  credentials read-only, cache freshness explicit, and widgets optional.
- **UI/daemon release skew:** negotiate versions and reject incompatible changes;
  keep generated types and protocol contract tests in the workspace.
- **Preview races and stale output:** one hardware-preview lease, expected config
  revisions, bounded queues, deterministic arbitration, and automatic restoration.
- **Scope growth:** Framework-only hardware, guided layouts, built-in widgets,
  local-only control, and no plugin platform in v1.

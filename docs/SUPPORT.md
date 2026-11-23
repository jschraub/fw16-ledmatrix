# Capability evidence and qualification

V1 targets Linux only. Windows rows record portability evidence, not v1 support
or release requirements (post-v1 development #42, support qualification #43).
A successful build is not a claim that all desktop,
driver, or panel capabilities work. The daemon `--probe` and desktop capability
screen distinguish available, unavailable, error, and not verified. Available
means a prerequisite was observed; it does not mean a full adapter is qualified.

| Environment | Evidence | Still unverified |
| --- | --- | --- |
| Arch Linux x86_64 (development machine) | Native builds, IPC/runtime tests, executable capability probe | No panels; no accessible graphical session for native-window checks |
| CachyOS Linux x86_64 / Hyprland (2026-11-23) | Both panels visually identified with firmware 0.20; distinct clock/battery roles and one/two-panel daemon-restart persistence; independent sleep/wake and relative brightness; AC transitions; preview expiry/cancel/window-close restoration | Initial identification/write failures remain unexplained; module removal interrupted keyboard/touchpad; reorder/reconnect/suspend qualification, full optics and brightness calibration pending |
| Ubuntu 24.04 x86_64 | CI build and automated tests | Interactive desktop features and real USB panels |
| Windows hosted runner | CI build, IPC security/restricted-token tests | Actual Windows 11 laptop, brightness/audio/session APIs, USB panel identity |

No Windows 11 hardware-support promise is made yet. Choose the published runtime
support matrix only after interactive qualification. Keep serialport for serial
enumeration/transport, sysfs for Linux power/backlight, bounded pactl queries for
Linux audio, and platform-specific Windows interfaces. Probe prerequisites before
adding feature controls. Failed/missing probes never masquerade as zero readings.

Credential probes inspect existence only; they do not open credentials, export
home paths, refresh tokens, or query provider endpoints. Display/session event
coverage remains unverified. Observed bay mappings below do not establish
identity persistence across USB reordering, reconnect or suspend.

## Initial CachyOS hardware session — 2026-11-23

**Lock-content optical qualification (#26):** Both firmware-0.20 panels remained
installed. The owner confirmed left clock / right blank while Hyprland was
locked, then left battery / right clock after unlock. Temporary saved presets
were reverted and original config contents verified at revision 8. Linux
fixtures separately cover quadratic mapping, DPMS sleep precedence over a
manual override, Apply while dark, preview cancellation across suspend, and
restoration of latest saved output. Windows adapters have GNU-target compile
and Clippy evidence only; Windows 11 interactive/hardware results remain open.

**Live display-off and suspend follow-up (#26):** With the explicit `eDP-2`
sleep policy temporarily previewed, both panel levels became off while the
session remained unlocked and the suspend epoch remained 0. The owner saw
both panels go dark and normal roles return after display wake and preview
cancellation. An independently approved system suspend then advanced the epoch
from 0 to 2. Both modules reopened with firmware 0.20; the owner confirmed left
battery, right clock and working keyboard/touchpad after wake/unlock. The saved
revision-8 configuration was identical. This is one installed two-panel cycle,
not replug qualification or proof of parser recovery.

**Linux lifecycle probes (#26):** A read-only CLI sample reported internal
connector `card1-eDP-2` DPMS On, logind's session lock hint clear, and system
sleep preparation false. `tools/check-lifecycle-signals.py` exercised the same
binary with synthetic DRM and logind responses: independent changes, overlap,
display-only DPMS Suspend, unknown responses, and recovery. These are point-in-time
capability samples; they do not establish delivery of a real lock, display-off,
suspend, or resume event, or trigger panel policy.

**Live DPMS cycle (#26, `07e7938`):** With only `eDP-2` active, a Hyprland
display-off request followed by an automatic display-on request produced
On → Off → On in both compositor status and the daemon CLI's DRM probe.
The logind lock hint was clear, `PreparingForSleep` was false, and actual
backlight brightness was 55/100 in all three samples. A separate user timer
was armed for backup wake before blanking. This verifies detection of a real
display-power transition on this setup, not panel sleep or lock/suspend policy.

**Live session lock (#26, `7ed7f14`):** After `loginctl lock-session` on the
active Wayland session, `hyprctl locked` first reported true at +0.32 s and
false at +6.40 s. In the samples, logind's `LockedHint` stayed clear, connector
DPMS stayed On and `PreparingForSleep` stayed false. A clear logind hint cannot
be used to infer an unlocked compositor here. The follow-up Linux probe reports
the Hyprland lock sample separately; it does not change panel output.

**Linux inventory follow-up (#31, `74a7a0c`, 2026-11-23).** The interactive
session ran CachyOS rolling, kernel `7.3.5-2-cachyos` (x86_64), and Hyprland
`v0.57.2` on Wayland. Hyprland reported one display: internal `eDP-2`, BOE
NE160QDM-NZ6, 2560×1600 at 165 Hz, scale 1. Both Framework USB interfaces
(`32ac:0020`) used `cdc_acm`. With the qualification daemon active and the POC
service inactive, a separate `--probe` run detected two panels, a sysfs
backlight and battery, a reachable audio server and a reachable user systemd
manager. Both credential presence checks reported available (existence only).
The probe reported bay identity, lock/display-off/suspend events and snapshot
storage as not verified. No suspend or module removal was performed.

At `2f06bf6` the user ran **Capabilities → Probe this machine** in the native
desktop against the running qualification daemon. The displayed platform,
two-panel count, and all ten capability states and descriptions matched the CLI
probe above. This verifies the populated Linux report.

At `76b3052` a second native desktop used an isolated endpoint with no daemon.
After clicking **Probe this machine**, the user confirmed the message
“Capability probe failed. Check the daemon connection and retry.” The test
desktop was then closed; the original daemon and desktop remained active.
This verifies an unreachable daemon, not an absent-panel report from a
reachable daemon.

At `6be04ff`, a separate native desktop connected to a reachable daemon with
an empty `/sys/class/tty` and a private `/dev`. The user confirmed that the
probe succeeded, reported `0 physical panels detected`, and showed
`panels: unavailable` with “No Framework LED panels detected. Simulated
development remains available.” Both modules stayed installed and under the
original daemon's ownership. This verifies the zero-panel screen without
testing physical removal or reconnect.

Related tickets: #31 (capability evidence), #32 (clock optics/latency),
#33 (battery/power), #34 (discovery/identification), #35 (persistent assignment),
#36 (recovery), #37 (brightness/sleep), #38 (temporary previews).

- Checkout: `dc59d07c5c90aca877d03837f09b44abbdf4ceca` with local Clippy and
  lifecycle-test fixes. The first successful right-panel identification used temporary stage
  logging only; no serial behavior change was made.
- Environment: CachyOS rolling, Linux x86_64, Hyprland. The probe detected two
  Framework modules, a sysfs backlight and battery, reachable audio and user
  systemd. These are prerequisites, not completed adapter qualification.
- The existing Python `matrixd.service` was backed up and stopped. Rust used
  separate configuration, IPC and feeds. At first launch no physical assignments
  were saved; later assignment checks are recorded below.
- Initial first-panel identification failed: the user saw old POC context/weekly
  content rather than an outline, followed by blank after about a minute. The UI
  reported reconnecting and no firmware. A daemon-IPC retry failed after 3.402
  seconds and reported a quarantined connection; permissions allowed access and
  no competing serial owner was found. The underlying cause is unresolved.
- A later attempt after a daemon restart succeeded. The native UI reported
  `/dev/ttyACM0`, firmware **0.20**, and sleeping by configured policy. The user
  confirmed the **right-hand panel** displayed an outline for approximately two
  seconds, then went blank. Diagnostics confirmed open/wake/firmware, brightness
  and outline writes completed. The user later clarified that no physical
  disconnect/reconnect was performed for that right-panel success: identification
  was simply retried after the daemon restart. This does not establish a safe
  recovery procedure following a potentially partial write.
- Observed right-panel location:
  `/sys/devices/pci0000:00/0000:00:08.3/0000:c4:00.0/usb3/3-3/3-3.3/3-3.3:1.0`.
  This is one observed mapping, not proof of persistence across restarts or
  enumeration changes. Port numbering must not be used as bay identity.
- Cancellation on the same right-hand panel passed after temporary diagnostics
  were removed: the user clicked Cancel identification and observed immediate
  blank output with no UI error. This is an interactive observation, not a
  measured restoration latency.
- Closing the native window during identification also returned the right-hand
  panel to blank, as observed by the user. After closure, systemd confirmed the
  desktop had exited and the same daemon process remained running; the POC was
  still stopped. This verifies unassigned-panel restoration and independent
  lifetimes for this scenario, not continued assigned output after UI closure.
- Initial single-panel assignment/clock check passed: following the guided
  configuration steps, the user confirmed the right-hand panel showed the
  current clock upright and readable and the left-hand panel remained blank.
  The requested configuration was logical `right` only, clock/gauge template
  with the gauge slot empty, manual brightness 32, and the observed right-bay
  location selected explicitly. Persistence and continued assigned output after
  closing the UI were not established by that initial check. This is not
  brightness calibration.
- Assigned-output UI-close check passed: the user closed the native window and
  confirmed the right-panel clock stayed lit and advanced through the next
  minute. Systemd confirmed the desktop was inactive, the daemon still running,
  and the POC still stopped. The user observed the physical minute change a few
  seconds behind the topbar clock; latency was not instrumented and the topbar's
  update timing was not measured. Clock/update latency remains to be quantified
  under #32/#36.
- Before the persistence restart check, the isolated on-disk configuration was
  inspected: revision 1, one logical `right` panel bound to the observed right-bay
  location, clock/gauge template with an empty gauge slot, clock intensity 180,
  manual brightness 32, sleep disabled, and no per-panel brightness override.
- Single-panel daemon-restart persistence check passed: after the daemon was
  restarted and the desktop reopened, the user confirmed the right-panel clock
  returned automatically with the left panel blank, without Apply. Loading saved
  configuration showed revision 1 and manual brightness 32. This demonstrates
  saved output restoration at the same observed location for one daemon restart;
  it does not qualify reboot, port reordering, suspend or reconnect recovery.
- First attempt to identify the second device failed: the user observed the
  left-hand panel wake with stale POC content (03:01 and a full vertical bar),
  then blank after approximately 30–60 seconds, with no outline. The UI reported
  reconnecting and no firmware for `/dev/ttyACM1`, location
  `/sys/devices/pci0000:00/0000:00:08.3/0000:c4:00.0/usb3/3-4/3-4.2/3-4.2:1.0`.
  Treat this as a candidate left-bay mapping pending successful identification.
- The Rust firmware handshake used one query and a 250 ms `read_exact`, unlike
  the POC's bounded repeated queries and longer reply window. Two regression
  scenarios (missing first reply; partial reply interrupted by a timeout) failed
  against that implementation. Local software now retries completely unanswered
  queries up to three times, allows a two-second reply window per query, and
  retains partial bytes within the window. Partial query writes, disconnects,
  incomplete and invalid replies stop the exchange. All 65 Rust tests,
  formatting and workspace Clippy passed on the pinned toolchains. The initial
  physical failures have not been conclusively attributed to this gap; hardware
  verification of the revised handshake on a reset left panel is recorded below.
- First left-panel retry with the revised handshake still failed: no outline,
  old image retained, and the UI reported a quarantined write failure with no
  firmware shown. Since failure handling clears cached firmware, the UI's
  missing firmware field does not establish that the query failed. The retry
  change is not a confirmed physical fix; stage-specific write diagnostics and
  a physical reconnect are needed before the next attempt.
- After the user explicitly disconnected the left module for five seconds and
  reconnected it, one controlled identification request through the existing
  daemon succeeded in 3.401 seconds. It targeted the candidate left-bay location,
  now `/dev/ttyACM1`, and returned firmware 0.20 and sleeping by configured policy.
  Stage diagnostics reported successful brightness and outline writes, followed
  by all nine restore columns and the commit command; individual restore writes
  took 0–25 ms. The right panel remained open with firmware 0.20 according to the
  device reply. The user confirmed a visible left-panel border for about two
  seconds, then blank, while the right panel kept its clock. This establishes the
  observed left-bay mapping for this session. This is evidence
  of a successful attempt after physical reset, not proof the earlier failure's
  cause was fixed or that automatic recovery is qualified.
- Number/two-gauges optical follow-up (#32, `982462b`): with the OS reporting
  100% battery, the user previewed battery / empty / battery on the left panel
  and confirmed the `XX` overflow marker was readable and the gauge bar was on
  the right side. The user also confirmed the left panel returned to its usual
  battery gauge after preview expiry. The saved configuration remained at
  revision 4 with left `full_gauge`. Restoration latency was not measured, and
  right-panel clock continuity was not separately confirmed for this run.
- Initial two-panel routing check passed: after adding logical `left` with a
  full-gauge battery source, binding it explicitly to the observed left-bay
  location, and applying with manual brightness 32, the user confirmed a battery
  bar on the left and the existing clock on the right. This establishes distinct
  roles in the current session, not charge accuracy, brightness calibration,
  two-panel restart persistence, or reconnect/suspend recovery.
- Assigned-content identification restoration passed on the left panel: the user
  confirmed a visible outline followed by restoration of the battery gauge,
  while the right clock remained visible. The native Devices UI reported
  `/dev/ttyACM1` open and firmware 0.20 at the observed left-bay location.
- Before the two-panel restart check, on-disk revision 2 was inspected: logical
  `right` retained the clock-only layout and right-bay binding; logical `left`
  used a full battery gauge and the left-bay binding. Both inherited manual
  brightness 32, with sleep disabled and no per-panel overrides.
- Two-panel daemon-restart persistence check passed: after the restart, the user
  confirmed both the left battery gauge and right current clock returned
  automatically without Apply. This qualifies one restart at the same observed
  locations, not port reordering, reboot, unplug or suspend recovery.
- Independent left-panel sleep check passed: after enabling a left-panel
  brightness override with explicit off/sleep and applying, the user confirmed
  the left panel went blank while the right clock stayed visible.
- Independent left-panel wake/brightness-inheritance check passed: after removing
  the left brightness override and applying, the user confirmed the battery gauge
  returned while the right clock stayed visible. This verifies one off-to-inherit
  transition at manual brightness 32, not min/max calibration or repeated-cycle
  reliability.
- Initial battery-source comparison passed: sysfs reported BAT1 capacity 33,
  status Charging, and ACAD online 1; the user confirmed the native UI showed
  approximately the same percentage, charging and AC connected. This is one
  charging-state comparison, not independent instrumentation of charge accuracy
  or evidence for unplug/resume, thresholds, or source-error recovery.
- AC-disconnect transition passed: after unplugging the charger and waiting
  approximately ten seconds, the user confirmed the UI showed AC disconnected
  and not charging while both panel roles remained visible. Subsequent sysfs
  reads showed BAT1 capacity 36, status Discharging, and ACAD online 0. This is
  one observed transition, not a measured event-delivery latency.
- AC-reconnect transition passed: after reconnecting the charger and waiting
  approximately ten seconds, the user confirmed charging and AC connected in
  the UI, with both panel roles still visible. Sysfs independently reported
  Charging and ACAD online 1. This completes one unplug/replug power-source
  cycle; battery threshold, source failure and suspend/resume checks remain open.
- Temporary hardware-preview expiry passed: with an unsaved draft enabling
  explicit left-panel sleep, the user started the ten-second preview and
  confirmed the left panel went blank temporarily, then automatically returned
  to its saved battery gauge while the right clock stayed visible. No Apply was
  requested. This verifies one expiry/restoration scenario, not preview
  cancellation, owner death, concurrent Apply or competing-owner handling.
- Temporary hardware-preview cancellation passed: using the same unsaved
  left-sleep draft, the user started another preview, cancelled after the left
  panel went blank, and confirmed its battery gauge returned shortly after
  cancellation rather than waiting for expiry. No Apply was requested. This is
  an observed restoration, not an instrumented latency measurement.
- Competing preview owners (#38): two independent IPC clients requested the
  saved layout on the running two-panel daemon. While A's 15-second lease was
  active, B's request returned `busy` and B's cancellation returned `not_owner`.
  After A closed its socket, a subscriber saw preview inactive 1.27 seconds
  after lease start, before expiry. B then acquired and cancelled a new lease.
  Saved configuration remained identical at revision 4; both ports reported
  open with firmware 0.20. This tests owner disconnect and exclusivity, not
  optical restoration latency or a forced native-desktop crash.
- Concurrent Apply (#38, isolated Linux daemon): `python3 tools/check-preview-apply.py`
  ran a real daemon with private config/feeds/IPC, private `/dev` and an empty
  tty inventory (requires `bwrap` and `cargo build -p matrix-daemon`). A second
  client saved revision 1 during a preview. A fresh post-Apply tick still
  showed the preview brightness, pixels and fixture binding. Natural expiry,
  explicit cancellation, and killing the owning IPC client each restored the
  newly saved output, including its changed binding; the cancellation case
  restored explicit off and blank pixels. Each result survived daemon restart.
  No physical routing, native UI crash, or optical latency was tested.
- Physical concurrent Apply (#38): starting at revision 4, the daemon reported
  a blank left preview frame during a ten-second lease. A second client saved a
  left clock while the lease was active. The observer acknowledged seeing the
  left clock 1.99 s after the lease-end reply (10.03 s from start to reply).
  The 1.99 s includes human reaction and dialog handling, not just LED latency.
  The client then restored the original configuration at revision 6 and checked
  its contents against revision 4. The observer confirmed the left battery
  gauge returned after rollback and the right clock stayed visible throughout.
- Closing the native window during a left-sleep preview restored the battery
  gauge immediately according to the user. Systemd confirmed the desktop exited,
  the same daemon process remained active, and the POC remained stopped. This
  tests normal window closure, not a forced UI crash; the restoration latency
  was not instrumented.
- Linux display-follow (#26, `40e6a65`): a 15-second preview paired internal
  `eDP-2` with `nvidia_wmi_ec_backlight`, using limits 3–215 on the right
  clock and a manual override of 32 on the left gauge. The observer saw the
  right clock brighten at 55% screen brightness, dim when the screen dropped
  to 25%, and return to normal after expiry. The left gauge stayed steady.
  Subscriber samples at 0.13, 5.13 and 15.13 seconds reported right levels
  120, 56 and 32 respectively, and left 32 at each sample. These are daemon
  timings, not optical latency. Requested and actual screen brightness returned
  to 55/100; saved revision 6 and its contents were unchanged.
- Volume feedback (#27, `1b840e5`): the observer confirmed the dim right-panel
  gauge appeared at brightness 3, its clock returned, and the left gauge stayed
  unchanged. The subscriber detected the gauge but did not capture a return
  frame within its observation window. This is a visual pass, not a measurement
  of six-second expiry. Original config contents were restored at revision 10;
  a later subscriber sample showed right brightness 32 and no active preview.
- Brightness feedback (#27): brightening the screen from 55% to 75% without
  the idle-dim marker showed a gauge on the right, then the clock returned.
  Dimming to 35% with the marker set lowered right-panel brightness to 37
  without displaying a gauge. The observer confirmed both transitions and
  that the left gauge stayed steady. This tests the marker, not idle detection
  itself. The screen returned to 55% and the prior settings, including the
  three-second right-panel volume gauge, were restored at revision 13.
- After `574f09b`, the observer reported that volume feedback on the physical
  panel was noticeably more responsive than the previous one-to-two-second
  delay. The isolated fixture measured 23–25 ms to the software gauge;
  key-to-LED latency was not instrumented.
- Right-panel sparse clock: at preview brightness **3** with clock intensity
  **180**, the user found the clock very dim but readable. They observed it
  return to its previous brightness after the ten-second preview. Saved revision
  4 still specified manual brightness 32. This is one observer's result on the
  right module; left-gauge continuity was not reported for this run.
- Independent brightness preview passed: with an unsaved left-panel override at
  manual brightness 64 and sleep disabled, the user confirmed only the left
  battery gauge became brighter, then returned to its saved brightness after
  the ten-second preview expired. The right clock's brightness stayed unchanged.
  This verifies a visible relative change at 32/64, not a calibrated visibility
  floor, photometric linearity, or min/max behavior.
- Identification-during-preview check passed for the observed sequence: during
  an unsaved left-panel brightness-64 preview, the user saw the left outline,
  then the battery gauge apparently at brightness 64, followed one or two seconds
  later by its saved brightness 32. Right-panel continuity was not separately
  confirmed for this attempt. This is consistent with the still-active preview
  resuming after identification and
  then expiring; intermediate brightness was judged visually, not measured.
  Concurrent Apply, competing owners and forced UI-crash behavior remain pending.
- Live left-module removal test was stopped: the user reported that the keyboard
  and touchpad did not work with one module disconnected. The user reconnected
  the left module and confirmed that both keyboard and touchpad worked again.
  The mechanism remains unverified; this is a qualification blocker, not a
  successful panel-output recovery check. Do not repeat physical module removal
  in this setup without first establishing a supported removal procedure and an
  independent input method.
- Read-only device inspection after reconnection found both expected physical
  locations present at `/dev/ttyACM0` and `/dev/ttyACM1`, each reporting firmware
  0.20 and open. The same daemon process remained active and the POC was stopped.
  This does not prove the daemon observed a missing module during removal or
  visually establish restored panel output.
- The user subsequently confirmed both assigned displays were visible again:
  left battery gauge and right current clock. Input and output were restored at
  the session checkpoint. Physical-removal qualification remains blocked because
  keyboard/touchpad input was lost during the attempted test; no further module
  removal was requested.
- Automated validation before hardware: Rust 1.94.0, Node 24.15.0, npm 11.16.0;
  59 Rust, 13 producer and 23 UI tests passed; formatting, workspace Clippy,
  frontend and native debug no-bundle builds passed; generated bindings unchanged.
- Pause checkpoint: isolated saved configuration revision 4, both inherited
  manual brightness 32, no overrides, explicit sleep disabled. Rust daemon and
  desktop left running, POC stopped but still enabled. Resume instructions and
  the official firmware-release check are in HARDWARE-HANDOFF.md.
- Pending: complete OS/kernel/display/driver inventory, reorder/reconnect
  persistence, brightness calibration, remaining power/preview scenarios and
  recovery checks. Tickets remain open; Windows hardware is untested.

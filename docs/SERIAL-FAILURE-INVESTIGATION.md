# Serial-failure investigation — 2026-11-23

Related: #34 / #36. Hardware baseline: `b3c0415`.

## Result so far

Linux pseudo-terminal fault injection reproduces a 250 ms write timeout through
the real `serialport` transport and identification path. It does **not** reproduce
or explain the physical panel failure. No physical ports were opened, services
started, modules removed, or firmware changed during this investigation.

Before diagnostics, the stalled brightness command returned only
`TimedOut: Operation timed out`; the diagnostic assertion failed. The same
scenario now reports:

```text
operation=write stage=identify-brightness command=0x00 column=None written=0/4 elapsed_ms=250 kind=TimedOut os_error=None
```

A second scenario accepts the brightness command and one outline byte before
stopping PTY output. It reports `stage=identify-outline command=0x06 written=1/42`
and never calls restoration. Kernel output flow control (`TCOOFF`) supplies
deterministic backpressure; it is not a model of the USB firmware or proof of
which driver state caused the physical failure.

Run the software-only loop from the repository root:

```sh
cargo test --locked -p matrix-daemon --lib serial_fault_tests -- --nocapture
```

The tests allocate their own PTYs, require no panels, and complete in about
250 ms when run concurrently. Both scenarios use the unchanged 250 ms timeout.
Generic writer tests also cover interrupted/short writes, write-zero, error-kind
preservation, OS-error reporting and byte-progress reporting.

## Diagnostics for the next failing hardware attempt

Every protocol-command write now logs failures at its boundary: stage, command
opcode, greyscale column where applicable, host-accepted bytes, elapsed
milliseconds, error kind and optional OS error. Terminal firmware-read failures
log attempt, received-byte count, elapsed time for that reply window, and error
kind/OS error. Expected read timeouts within a reply window are not logged.

Diagnostics contain no frame payloads, device paths or raw error messages.
Successful writes are silent; host acceptance does not establish firmware
receipt or parser state. Logs must still be reviewed before sharing.

Timeouts, retry windows, command ordering, output flushing and quarantine policy
are unchanged. No recovery or parser-resynchronization claim follows from these
tests.

## Rust / POC boundary comparison

| Boundary | Rust | Preserved POC |
| --- | --- | --- |
| Write | `serialport`, 250 ms timeout, complete-command write loop | Blocking `os.write`; returned short-write count is not checked |
| Read | 250 ms calls within two-second firmware reply windows | Termios `VMIN=0`, `VTIME=5` (500 ms reads) |
| Open | 200 ms settle, wake, one-second settle, version query | Termios configuration and `TCIOFLUSH`, then open settle; wake separate |
| Partial write | Stops the exchange; established connection quarantined by caller | No equivalent partial-write accounting in `_send` |

Do not copy the POC's queue flushing or blocking writes as a presumed fix.

## Still needed

- Capture an actual failed panel operation; successful traces are insufficient.
- Distinguish write backpressure from terminal firmware-read failure and
  disconnect/driver errors using the new boundary records.
- Inspect current services and serial ownership before any hardware attempt.
  Keep physical removal paused because of the keyboard/touchpad input-loss
  blocker. Restart alone does not prove the firmware parser was reset.

Baseline Linux/Windows build-and-test CI for `b3c0415` succeeded:
[run 45155214863](https://github.com/jschraub/fw16-ledmatrix/actions/runs/45155214863).
This does not validate these uncommitted diagnostics or qualify hardware.

## Startup quarantine follow-up (#36)

The startup regression accepted one wake byte, timed out, then exercised the
normal retry gate six simulated seconds later. Before the fix it accepted four
additional wake bytes (five total). `Panel::open` had not yet cached the port,
so `Panel::failed` misclassified a command failure as an open failure.

The gate now quarantines typed command-write failures even during startup;
ordinary open/access errors still retry after five seconds. A failed enumeration
is no longer treated as an observed absence for registry cleanup. The regression
covers a partial wake and a partial firmware query. All 71 Rust tests and
workspace/all-target Clippy passed. The serial timeout remains 250 ms.

Physical follow-up on the same laptop: both installed panels had no visible
serial owner, and all three user services were inactive before launch. The new
daemon was started with the isolated revision-4 config and was the only visible
owner of both ports. Both reported open and firmware 0.20. One IPC identification
targeting the saved left binding completed in 3.147 seconds with restoration
reported and both ports still open. The daemon journal recorded no serial
failure. No optical check was made. This attempt did not reproduce the original
hardware failure or establish its cause. The instrumented daemon was left active.

## Local validation

- All 70 Rust tests passed (65 baseline plus five new tests).
- Both PTY fault-injection cases passed in 10 consecutive runs (20 checks).
- Workspace/all-target Clippy with warnings denied passed.
- Rust formatting and `git diff --check` passed.
- Frontend tests/build and native packaging were not rerun; no UI or contracts
  changed. Windows execution of these changes and physical failure capture remain
  pending.

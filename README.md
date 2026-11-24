# Framework LED Matrix

A configurable ambient display application for the Framework Laptop 16 LED
Matrix modules.

The native application uses a standalone Rust daemon and a React/TypeScript
Tauri desktop. **Linux is the only v1 release target.** Closing the UI leaves rendering and
sources running. **Development builds are available; this is not yet a qualified
packaged release.**

Implemented software paths:

- Private local IPC, capability reporting, durable validated configuration,
  Save/Discard drafts, revision conflicts and bounded temporary output previews.
- Guided one/two-panel layouts, live clocks and battery widgets, independent
  widget intensity, manual brightness limits/overrides and explicit sleep.
- Framework discovery/firmware identification, physical-location bindings,
  independent output routing, bounded retries and partial-write quarantine.
- Metadata-only Claude helper and OpenCode V2 plugin, provider/session selection,
  context/activity widgets, heartbeat expiry and integration health.

Linux display/session policies are implemented; tested hardware scenarios and
remaining limits are recorded below. Windows is post-v1:
[development #42](https://github.com/jschraub/fw16-ledmatrix/issues/42) and
[support qualification #43](https://github.com/jschraub/fw16-ledmatrix/issues/43).
Optional usage windows, per-panel charging and feedback effects, Arch packaging,
login startup and [one-way migration](docs/MIGRATION.md) are implemented.
Release qualification remains open. See the
[roadmap](https://github.com/jschraub/fw16-ledmatrix/issues/1),
[support evidence](docs/SUPPORT.md), [widget behavior](docs/WIDGETS.md), and
[integration setup](docs/INTEGRATIONS.md). Hardware follow-ups remain open rather
than treating simulated tests as physical evidence.

## Develop without panels

For the native Arch/CachyOS package candidate, see [Linux packaging](docs/PACKAGING.md).

Continuing on a laptop with panels? Start with the
[hardware-machine handoff](docs/HARDWARE-HANDOFF.md), including open tickets,
safe first-run steps and the current review environment.

At the root, start `cargo run --locked -p matrix-daemon`. In another terminal,
run `npm ci` followed by `npm run tauri -- dev` from `apps/desktop`.
The daemon remains running when you close the desktop window.

See [development instructions](docs/DEVELOPMENT.md) for prerequisites, tests,
endpoint overrides, and platform validation limits.

## Design review

[Signal Room workspace audit and screenshots](docs/design/workspace-audit.md):
Overview edits one bay; Configuration manages settings; Devices identifies
hardware; Integrations shows provider health; Diagnostics generates a support report.

See [PLAN.md](PLAN.md) for the approved architecture, release scope, milestones,
validation strategy, and remaining decisions.

## Existing proof of concept

The working Linux/Python implementation is preserved in [`poc/`](poc/), including
its integrations, installer, tests, hardware tools, and historical design notes.
See its [README](poc/README.md) and [status](poc/STATUS.md).

Validate without hardware:

```sh
cd poc
python3 -m unittest discover -s tests -t .
node --test tests/test_opencode_plugin.mjs
```

Keep the POC for reference. On migrated machines leave it disabled; the packaged
Rust service is the sole panel writer and the recovery target.

## License

[MIT](LICENSE).

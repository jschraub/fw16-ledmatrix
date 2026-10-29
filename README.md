# Framework LED Matrix

A configurable ambient display application for the Framework Laptop 16 LED
Matrix modules.

The native application uses a standalone Rust daemon and a React/TypeScript
Tauri desktop, targeting Linux and Windows. Closing the UI leaves rendering and
sources running. **Development builds are available; this is not yet a qualified
packaged release.**

Implemented software paths:

- Private local IPC, capability reporting, durable validated configuration,
  Apply/Cancel drafts, revision conflicts and bounded temporary output previews.
- Guided one/two-panel layouts, live clocks and battery widgets, independent
  widget intensity, manual brightness limits/overrides and explicit sleep.
- Framework discovery/firmware identification, physical-location bindings,
  independent output routing, bounded retries and partial-write quarantine.
- Metadata-only Claude helper and OpenCode V2 plugin, provider/session selection,
  context/activity widgets, heartbeat expiry and integration health.

Panel behavior and interactive OS support remain explicitly unqualified. Windows
stable bay identity is not implemented: unknown locations cannot be assigned.
Quota sources, display/session policies, overlays, packaging/startup, migration
and release qualification remain tracked work. See the
[roadmap](https://github.com/jschraub/fw16-ledmatrix/issues/1),
[support evidence](docs/SUPPORT.md), [widget behavior](docs/WIDGETS.md), and
[integration setup](docs/INTEGRATIONS.md). Hardware follow-ups remain open rather
than treating simulated tests as physical evidence.

## Develop without panels

At the root, start `cargo run --locked -p matrix-daemon`. In another terminal,
run `npm ci` followed by `npm run tauri -- dev` from `apps/desktop`.
The daemon remains running when you close the desktop window.

See [development instructions](docs/DEVELOPMENT.md) for prerequisites, tests,
endpoint overrides, and platform validation limits.

## Design review

See [PLAN.md](PLAN.md) for the approved architecture, release scope, milestones,
validation strategy, and remaining decisions.

## Existing proof of concept

The working Linux/Python implementation is preserved in [`poc/`](poc/), including
its integrations, installer, tests, hardware tools, and historical design notes.
See its [README](poc/README.md) and [status](poc/STATUS.md).

Run from that directory:

```sh
cd poc
python3 -m matrixd
```

Validate without hardware:

```sh
cd poc
python3 -m unittest discover -s tests -t .
node --test tests/test_opencode_plugin.mjs
```

An existing systemd installation points at the old checkout root. After updating,
re-run `./install.sh` **from `poc/`**, including your previous `--provider` option
if applicable, to update its working directory. External scripts that invoke the
old root-level installer also need to use `poc/install.sh`.

Do not run the proof of concept and the Rust daemon against the same
panels simultaneously. This reorganization does not install or restart anything.

## License

[MIT](LICENSE).

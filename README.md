# Framework LED Matrix

A configurable ambient display application for the Framework Laptop 16 LED
Matrix modules.

The next version is being designed around a standalone Rust daemon and a Tauri
desktop application using React and TypeScript, targeting Linux and Windows.
It will provide guided layouts, configurable widgets, brightness policies, and
live previews. **The initial hardware-free walking skeleton is implemented:** a
Rust daemon streams a simulated 9×34 clock panel to the React/Tauri desktop.
Configuration and physical-device support are not implemented yet.

## Develop without panels

At the root, start `cargo run --locked -p matrix-daemon`. In another terminal,
run `npm ci` followed by `npm run tauri -- dev` from `apps/desktop`.
The daemon remains running when you close the desktop window.

See [development instructions](docs/DEVELOPMENT.md) for prerequisites, tests,
endpoint overrides, and platform validation limits.

## Design review

See [PLAN.md](PLAN.md) for the proposed architecture, release scope, milestones,
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

Do not run the proof of concept and the future Rust daemon against the same
panels simultaneously. This reorganization does not install or restart anything.

## License

[MIT](LICENSE).

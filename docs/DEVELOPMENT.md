# Hardware-free development

Issue #9 supplies a walking skeleton, not the complete configuration application.
It opens **no serial ports**, reads no assistant credentials, and installs no
services. The panel is simulated; the local wall clock is real.

## Prerequisites

- Rust via rustup: the root toolchain file pins 1.94.0 with rustfmt and Clippy.
  System-managed Rust also works if it satisfies the workspace minimum, but does
  not automatically honor rustup's toolchain selection.
- Node: the root `.node-version` pins 24.14.0; npm 11.16.0 is the package manager.
- Linux: GTK 3, WebKitGTK 4.1, and the normal Tauri build dependencies. On Ubuntu:

  ```sh
  sudo apt-get install build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
  ```

- Windows: Microsoft C++ build tools (MSVC/Windows SDK) and WebView2 Runtime, plus
  Rust's MSVC host toolchain. No physical Framework panel is required.

## Run

In a terminal at the repository root:

```sh
cargo run --locked -p matrix-daemon
```

Leave it running. In another terminal:

```sh
cd apps/desktop
npm ci
npm run tauri -- dev
```

The window shows connection status, daemon/protocol version, capabilities, a
simulation badge, the live clock frame, and its update sequence. Clock digits
change each minute; sequence advances every second so liveness is visible. The
remaining rows are dark. React paints the exact row-major greyscale pixels sent
by Rust; it has no font or time-rendering logic.

Close the window and reopen it with the same command. The daemon must remain
running and the new window must receive its latest frame, not replay history.
Stop the daemon with Ctrl-C. An open UI should clear its stale panel, show the
connection failure, and offer Retry. Restart the daemon and retry explicitly.
Automatic reconnection/startup management is deferred to #10 and later slices.

`npm run dev` alone runs only Vite: a browser cannot access the native Tauri
bridge and will report a connection error. Use `npm run tauri -- dev` for the
complete application.

## Local endpoint

- Linux: `fw16-matrix/daemon.sock` beneath the user runtime directory, falling
  back to the user cache directory. Newly created parent directories are private
  and the socket is mode 0600.
- Windows: a local named pipe `fw16-matrix-<USERNAME>`; remote clients are rejected.
  Explicit Windows ACL/peer hardening and stronger identity are part of #10.
- Set `MATRIX_ENDPOINT` to the **same** socket path/full named-pipe name in both
  terminals for an isolated development instance. There is no TCP listener.

An occupied endpoint is never automatically unlinked/replaced. On Linux a hard
kill may leave a stale socket: verify no daemon is using it before removing that
specific socket yourself. Ctrl-C performs normal socket cleanup. Production
single-instance recovery and user-access hardening remain #10 work; this is a
local development skeleton, not a hardened release.

## Tests and builds

At the root (default Cargo members are the headless crates):

```sh
cargo fmt --all -- --check
cargo test --locked
cargo run --locked -p matrix-client --example export-types
```

The final command regenerates the committed TypeScript contract from Rust.
Do not edit generated bindings. CI verifies they remain synchronized.

In `apps/desktop`:

```sh
npm ci
npm test
npm run build
npm run tauri -- build --debug --no-bundle
```

After building frontend assets, root-level native lint checks are:

```sh
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The no-bundle build produces a native executable in the workspace target folder;
installers/signing are deferred. CI builds the release executable on Linux and
Windows. Tests include deterministic POC-compatible clock frames, bounded framing,
latest-frame subscriptions, two-client isolation, simulated UI feedback/retry,
and a real daemon subprocess surviving client close/reopen. Tests use isolated
socket/pipe endpoints and require no hardware, credentials, or elevated access.

## Manual qualification still required

Record the OS/toolchain and result when checking native window close/reopen and
Rust-to-Tauri-to-React output. Automated Rust process/client and mocked UI tests
do not establish that the complete native window works on an untested OS.
Windows builds and runtime checks must actually run on Windows before claiming
Windows qualification. Physical USB/firmware timing and LED visibility remain
separate hardware checks for later slices.

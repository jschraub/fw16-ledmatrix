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
Stop the daemon with Ctrl-C. An open UI should clear its stale panel, retain
caller-owned UI state, show a structured connection failure, and offer Retry.
Restart the daemon and retry explicitly. A stalled frame stream is also closed
after ten seconds rather than showing an indefinitely frozen live clock.

Alternatively, run `cargo build --locked -p matrix-daemon` first. When the desktop
reports that no daemon is listening, **Start daemon** launches the executable
beside the desktop binary and waits for readiness. It does not search PATH, use
a shell, install a service, or terminate a running process. The daemon keeps
running after the window closes. Incompatible, inaccessible, malformed, or
unresponsive endpoints block startup; they are not permission to replace a
process. Missing sibling binaries produce an actionable startup error. Automatic
restart, login startup, and packaging are still deferred.

The Configuration section loads a versioned saved document into a local draft.
Validate, Apply, and Cancel are explicit. Clock intensity changes demonstrate live
application without a daemon restart. The daemon validates and atomically saves
before publishing a new revision. Failed saves retain the draft and previous
applied state; invalid/corrupt/future-schema files are preserved at startup.
`MATRIX_CONFIG` overrides the config path for isolated development/tests; otherwise
it is `fw16-matrix/config.json` below the platform user configuration directory.

`npm run dev` alone runs only Vite: a browser cannot access the native Tauri
bridge and will report a connection error. Use `npm run tauri -- dev` for the
complete application.

## Local endpoint

- Linux: `fw16-matrix/daemon.sock` beneath the user runtime directory, falling
  back to the user cache directory. The parent must be a real directory owned by
  the current user with no group/other access; new directories use mode 0700.
  The socket uses mode 0600 and both sides check peer credentials. Symlinks and
  insecure existing directories/files are rejected, not silently repaired.
- Windows: a local named pipe `fw16-matrix-<current-user-SID>`. A protected DACL
  allows only that user's SID; remote clients/network pipe paths are rejected.
  The client also verifies the server process's user SID. Restricted-token access
  checks are included in Windows-only automated tests, without requiring another
  account or administrator privileges.
- Set `MATRIX_ENDPOINT` to the **same** socket path/full named-pipe name in both
  terminals for an isolated development instance. There is no TCP listener.

The default endpoint permits one daemon for the current user. Explicit endpoint
overrides intentionally permit isolated parallel development instances. On Linux
an OS-held exclusive lock protects the endpoint; after a crash, an owner-matching
stale socket can be recovered only while holding that lock and only after a
connection is refused. A live/unresponsive endpoint is never unlinked, including
an older daemon that does not implement the lock. Windows uses exclusive first
pipe-instance creation. No code kills or replaces an existing daemon. Ctrl-C
performs normal socket cleanup; lock files intentionally remain for future use.

The control protocol is now version 2. Update/restart both binaries together;
an incompatible running daemon must be stopped explicitly by the user. This is
still a development application, not a security-audited packaged release.

Messages are bounded to 64 KiB, initial handshakes to five seconds, concurrent
sessions to 32, and stalled writes to five seconds. The daemon retains only the
latest frame. The Tauri bridge allows one unacknowledged frame per subscription
and closes it after five seconds without a matching acknowledgement; stale
subscription/sequence acknowledgements cannot release a new frame. Thus a stuck
WebView cannot accumulate an unbounded frame-event queue.

Connection diagnostics use fixed error codes/actionable text, not raw OS errors,
endpoint paths, usernames, provider credentials, or malformed payloads. Raw server
error text is not trusted or forwarded. There is no network listener.

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
and real daemon subprocesses surviving client close/reopen, rejecting duplicate
instances, explicitly starting, and recovering from hard crashes. Additional
tests cover malformed/oversized messages, version mismatch, redaction, admission
limits, silent/slow clients, filesystem permissions, and stale-path safety. Tests use isolated
socket/pipe endpoints and require no hardware, credentials, or elevated access.

## Manual qualification still required

Record the OS/toolchain and result when checking native window close/reopen and
Rust-to-Tauri-to-React output. Automated Rust process/client and mocked UI tests
do not establish that the complete native window works on an untested OS.
Windows builds and runtime checks must actually run on Windows before claiming
Windows qualification. Physical USB/firmware timing and LED visibility remain
separate hardware checks for later slices.

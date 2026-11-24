# Move to the packaged Linux daemon

The Python daemon was a proof of concept. The Rust daemon is the product.
On a machine already migrated, leave the old unit disabled.

1. Back up the Rust configuration, if present (`~/.config/fw16-matrix/config.json`),
   and the old service unit and integration settings to a private directory.
   Keep these local: client settings can contain secrets. Do not rerun the POC installer.
2. Close any old desktop, then explicitly `systemctl --user disable --now matrixd.service`
   if that unit exists. Stop any standalone writer you launched. Check the
   Framework port names in Devices or `/dev/serial/by-path`. Run `fuser` on
   **each** matching port; resolve any remaining owner before proceeding.
3. Install the [Arch package](PACKAGING.md) and use
   `systemctl --user enable --now fw16-ledmatrix.service`. Check that its
   `MainPID` alone owns each assigned port. A missing or unassigned panel is
   not evidence of a competing writer. Launch the installed desktop without
   any handoff `MATRIX_CONFIG` or `MATRIX_ENDPOINT` overrides.
4. Existing Rust settings carry forward. Configure new panels explicitly by
   physical location, not tty enumeration order. Integration setup is optional:
   review existing client settings and replace a legacy producer intentionally
   using [the producer instructions](INTEGRATIONS.md). Do not enable both.

## Diagnose and recover in Rust

- **Connection failed:** check `systemctl --user status fw16-ledmatrix.service`.
  Use the package's matching desktop and daemon. Restart the Rust service if
  needed and choose **Retry connection**. A draft stays in the open desktop;
  **Refresh saved state (keep draft)** adopts the current revision for review.
  **Load saved configuration** deliberately replaces the draft.
- **Panel blank:** check Devices → Discover, then Configuration → Assignments.
  Confirm location, single ownership, brightness and sleep policy. A partial
  serial write is quarantined: don't force another writer or remove modules
  on a running laptop where removal has disrupted input.
- **Source unavailable:** check Configuration → Brightness for display fallback
  and Integrations for missing feeds, expired usage or unsupported login.
  Missing assistant usage is not zero. Unrelated panels can keep rendering.
- **Support:** Tab to Diagnostics → Generate redacted report. Review the JSON
  and choose Copy report, or focus the text field and press Ctrl+A, Ctrl+C.
  The report contains fixed status labels and counts, not paths, identities,
  credentials, raw exceptions, frames or conversation text.
- **Bad saved settings:** stop the Rust service, preserve the current file in
  a private backup, restore a known-good **Rust** configuration from this
  account, then start that same service and check ownership. Do not import
  POC configuration as Rust JSON or start the POC as a fallback.

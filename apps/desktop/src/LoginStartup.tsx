import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export function LoginStartup() {
  const [enabled, setEnabled] = useState<boolean | null>();
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  async function check(change?: boolean) {
    setBusy(true);
    try {
      setEnabled(await invoke<boolean | null>("login_startup", { enabled: change ?? null }));
      setFailed(false);
    } catch {
      setEnabled(undefined); setFailed(true);
    } finally { setBusy(false); }
  }
  return <details><summary>Login startup</summary>
    <p>Start the daemon automatically at future graphical logins. Changing this setting does not start or stop it now.</p>
    <button disabled={busy} onClick={() => void check()}>Check login startup</button>
    {typeof enabled === "boolean" && <button disabled={busy} onClick={() => void check(!enabled)}>
      {enabled ? "Disable login startup" : "Enable login startup"}
    </button>}
    <p aria-live="polite">{failed ? "Could not manage the packaged systemd user service. Check its status and retry." :
      enabled === null ? "Unavailable: install the Linux package on a systemd user session." :
      enabled === undefined ? "" : `Login startup is ${enabled ? "enabled" : "disabled"}.`}</p>
  </details>;
}

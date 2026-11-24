import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Snapshot } from "./bindings/Snapshot";
import type { DeviceReply } from "./bindings/DeviceReply";
import { diagnosticReport } from "./diagnosticReport";

export function DiagnosticExport({snapshot, failure}: {snapshot: Snapshot | null; failure?: string}) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  async function generate() {
    setBusy(true); setMessage("");
    let devices = null;
    try {
      if (snapshot) {
        const reply = await invoke<DeviceReply>("devices", {request:{type:"devices"}});
        if (!reply.message.startsWith("Serial enumeration failed")) devices = reply.devices;
      }
    } catch { /* Keep available source evidence if enumeration failed. */ }
    setText(diagnosticReport(snapshot, failure, devices));
    setBusy(false);
  }
  async function copy() {
    try { await navigator.clipboard.writeText(text); setMessage("Report copied."); }
    catch { setMessage("Select the report text and copy it with your keyboard."); }
  }
  return <section aria-label="Support report"><h2>Support report</h2>
    <button disabled={busy} onClick={() => void generate()}>Generate redacted report</button>
    {text && <><label>Redacted report (JSON)<textarea readOnly rows={10} value={text} onFocus={e => e.target.select()} /></label>
      <button onClick={() => void copy()}>Copy report</button>
      <p role="status">{message}</p></>}
    <p>For a missing panel, check Devices and its assignment. For a missing source, check Integrations or the display selection under Brightness.</p>
  </section>;
}

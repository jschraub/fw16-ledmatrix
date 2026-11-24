import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Snapshot } from "./bindings/Snapshot";
import type { DeviceReply } from "./bindings/DeviceReply";
import { diagnosticReport } from "./diagnosticReport";
import { useActionNotice } from "./useActionNotice";

export function DiagnosticExport({snapshot, failure}: {snapshot: Snapshot | null; failure?: string}) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useActionNotice();
  async function generate() {
    setBusy(true); setNotice(null);
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
    try { await navigator.clipboard.writeText(text); setNotice({tone:"success",text:"Report copied."}); }
    catch { setNotice({tone:"error",text:"Select the report and copy with your keyboard."}); }
  }
  return <section className="workspace-bottom" aria-label="Support report"><div className="section-line"><h3>Support report</h3>
    <button disabled={busy} onClick={() => void generate()}>{busy?"Generating…":"Generate redacted report"}</button></div>
    {text && <><label>Redacted report (JSON)<textarea readOnly rows={10} value={text} onFocus={e => e.target.select()} /></label>
      <button onClick={() => void copy()}>Copy report</button>
      <div className="action-notice" aria-live="polite" data-tone={notice?.tone}>{notice?.text}</div></>}
    {!text && <p className="muted">Share device and source status without paths, credentials or session IDs.</p>}
  </section>;
}

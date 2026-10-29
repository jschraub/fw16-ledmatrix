import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CapabilityReport } from "./bindings/CapabilityReport";

export function Capabilities() {
  const [report, setReport] = useState<CapabilityReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  async function refresh() {
    setBusy(true); setFailed(false);
    try { setReport(await invoke<CapabilityReport>("probe")); }
    catch { setFailed(true); }
    finally { setBusy(false); }
  }
  return <section aria-label="Platform capabilities">
    <h2>Capabilities</h2>
    <button onClick={() => void refresh()} disabled={busy}>{busy ? "Probing…" : "Probe this machine"}</button>
    {failed && <p role="alert">Capability probe failed. Check the daemon connection and retry.</p>}
    {report && <><p>{report.os} · {report.architecture} · {report.desktop} · {report.panel_count} physical panels detected</p>
      <dl>{report.capabilities.map((capability) => <div key={capability.name}>
        <dt>{capability.name}: <strong>{capability.state.replaceAll("_", " ")}</strong></dt>
        <dd>{capability.detail}</dd>
      </div>)}</dl></>}
  </section>;
}

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
    <div className="section-line"><h3>Machine capabilities</h3>
    <button onClick={() => void refresh()} disabled={busy}>{busy ? "Probing…" : "Probe this machine"}</button>
    </div>{failed && <p role="alert">Capability probe failed. Check the daemon connection and retry.</p>}
    {report && !failed && <><p className="muted">{report.os} · {report.architecture} · {report.desktop} · {report.panel_count} physical panels detected</p>
      {report.capabilities.map(c=><details key={c.name} className="capability-row"><summary>{c.name}
        <span className="state-label" data-tone={c.state==="available"?"success":c.state==="error"?"error":undefined}>{c.state.replaceAll("_"," ")}</span>
      </summary><p>{c.detail}</p></details>)}</>}
  </section>;
}

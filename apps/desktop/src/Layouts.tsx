import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Config } from "./bindings/Config";
import type { Frame } from "./bindings/Frame";
import type { PanelLayout } from "./bindings/PanelLayout";
import type { Presentation } from "./bindings/Presentation";
import type { Widget } from "./bindings/Widget";
import type { LockContent } from "./bindings/LockContent";
import { LayoutComposer } from "./LayoutComposer";

function widget(presentation: Presentation): Widget { return { source: presentation === "clock" ? "clock" : "battery", provider: "claude", session_id: null, presentation, intensity: null, reversed: false, hide_leading_hour_zero: false, warning_threshold: null }; }

export function Layouts({ config, change }: { config: Config; change: (config: Config) => void }) {
  const panels = config.panels ?? [];
  const [selected,setSelected]=useState<string>();
  const names=panels.map(p=>p.name).sort();
  const side=selected && names.includes(selected)?selected:names[0];
  function update(index: number, panel: PanelLayout) { change({ ...config, panels: panels.map((p, i) => i === index ? panel : p) }); }
  function slot(p: number, s: number, value: Widget | null) { update(p, { ...panels[p], slots: panels[p].slots.map((old, i) => i === s ? value : old) }); }
  return <section aria-label="Panel layouts">
    {names.length>1 && <div className="side-picker" role="group" aria-label="Panel to configure">
      {names.map(name=><button key={name} type="button" aria-pressed={side===name} onClick={()=>setSelected(name)}>{name} panel</button>)}
    </div>}
    {panels.map((panel, p) => panel.name===side && <article className="config-panel" key={panel.name} aria-label={`${panel.name} panel layout`}>
      <LayoutComposer panel={panel} update={patch=>update(p,{...panel,...patch})}/>
      <details><summary>Lock behavior and widget options</summary>
      <label>When locked <select aria-label={`When locked for ${panel.name}`} value={panel.when_locked ?? ""}
        onChange={e => update(p, { ...panel, when_locked: (e.target.value || undefined) as LockContent | undefined })}>
        <option value="">Keep normal layout</option><option value="clock">Clock only</option>
        <option value="battery">Battery gauge</option><option value="blank">Blank panel</option>
      </select></label>
      {panel.slots.map((value, s) => value && <section key={s} className="widget-options"><h4>{panel.template==="number_two_gauges"?["Number","Left bar","Right bar"][s]:value.presentation==="clock"?"Clock":"Bar"}</h4>
        <label>Intensity <input type="number" min={1} max={255} placeholder="inherit" value={value.intensity ?? ""}
          onChange={(e) => slot(p, s, { ...value, intensity: e.target.value === "" ? null : Number(e.target.value) })} /></label>
          {(value.source === "context" || value.source === "activity") &&
              <label>Session pin <input placeholder="Latest active session" value={value.session_id ?? ""} maxLength={128} onChange={(e) => slot(p, s, { ...value, session_id: e.target.value || null })} /></label>
          }
          {value.presentation === "gauge" && <label><input type="checkbox" checked={value.reversed} onChange={(e) => slot(p, s, { ...value, reversed: e.target.checked })} /> Fill downward</label>}
          {value.source !== "clock" && <label>Warning threshold ({value.source === "battery" ? "at or below" : "at or above"})
            <input type="number" min={0} max={100} value={value.warning_threshold ?? ""} placeholder="disabled"
              onChange={(e) => slot(p, s, { ...value, warning_threshold: e.target.value === "" ? null : Number(e.target.value) })} /></label>}
          {value.presentation === "clock" && <>
            <label><input type="checkbox" checked={value.hide_leading_hour_zero ?? false} onChange={(e) => slot(p, s, { ...value, hide_leading_hour_zero: e.target.checked })} /> Hide leading hour zero</label></>}
      </section>)}</details>
      {panels.length === 2 && <button type="button" onClick={() => change({ ...config, panels: panels.filter((_, i) => i !== p) })}>Remove {panel.name}</button>}
    </article>)}
    {panels.length === 1 && <button type="button" onClick={() => {
      const name=panels[0].name==="left"?"right":"left";
      change({ ...config, panels: [...panels, { name, binding: null, brightness_override: null, template: "clock_gauge", slots: [widget("clock"),widget("gauge")] }] });
      setSelected(name);
    }}>Add second panel</button>}
  </section>;
}

export function DraftPreview({ config }: { config: Config }) {
  const [frames, setFrames] = useState<Frame[] | null>(null);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const generation = useRef(0);
  useEffect(() => { generation.current++; setFrames(null); setMessage(""); setBusy(false); return () => { generation.current++; }; }, [config]);
  async function preview(sample: boolean) {
    const current = ++generation.current;
    setBusy(true);
    try { const result = await invoke<Frame[]>("render_draft", { config, sample }); if (current === generation.current) { setFrames(result); setMessage(sample ? "SAMPLE DATA · not live readings" : "Current readings; unavailable sources remain blank"); } }
    catch { if (current === generation.current) { setFrames(null); setMessage("Preview rejected or daemon unavailable. Validate the draft and retry."); } }
    finally { if (current === generation.current) setBusy(false); }
  }
  return <section aria-label="Draft preview">
    <button type="button" disabled={busy} onClick={() => void preview(true)}>Preview with sample data</button>
    <button type="button" disabled={busy} onClick={() => void preview(false)}>Preview with current readings</button>
    <p>{message}</p><div className="draft-panels">{frames?.map((frame, i) => <figure key={i}><figcaption>{config.panels[i].name}</figcaption>
      <div className="panel" role="img" aria-label={`Draft preview for ${config.panels[i].name}`}><div className="pixels" aria-hidden="true">{frame.pixels.map((pixel, n) => <span key={n} className="pixel" style={{ opacity: pixel ? pixel / 255 : 0.06 }} />)}</div></div>
    </figure>)}</div>
  </section>;
}

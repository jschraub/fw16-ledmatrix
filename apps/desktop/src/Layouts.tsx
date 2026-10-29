import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Config } from "./bindings/Config";
import type { Frame } from "./bindings/Frame";
import type { PanelLayout } from "./bindings/PanelLayout";
import type { Template } from "./bindings/Template";
import type { Presentation } from "./bindings/Presentation";
import type { Source } from "./bindings/Source";
import type { Widget } from "./bindings/Widget";

const templates: Record<Template, Presentation[]> = { clock_gauge: ["clock", "gauge"], number_two_gauges: ["number", "gauge", "gauge"], full_gauge: ["gauge"] };
const sources: Source[] = ["battery", "context", "activity", "five_hour", "weekly"];
function widget(presentation: Presentation): Widget { return { source: presentation === "clock" ? "clock" : "battery", presentation, intensity: null, reversed: false, hide_leading_hour_zero: false }; }

export function Layouts({ config, change }: { config: Config; change: (config: Config) => void }) {
  const panels = config.panels ?? [];
  function update(index: number, panel: PanelLayout) { change({ ...config, panels: panels.map((p, i) => i === index ? panel : p) }); }
  function slot(p: number, s: number, value: Widget | null) { update(p, { ...panels[p], slots: panels[p].slots.map((old, i) => i === s ? value : old) }); }
  return <fieldset><legend>Guided layouts</legend>
    <p>Templates fix geometry to 9 × 34. Changing a template resets its slots. Missing readings leave slots blank.</p>
    {panels.map((panel, p) => <fieldset key={panel.name}><legend>{panel.name}</legend>
      <label>Template for {panel.name} <select value={panel.template} onChange={(e) => { const template = e.target.value as Template; update(p, { ...panel, template, slots: templates[template].map(widget) }); }}>
        {Object.keys(templates).map((t) => <option key={t} value={t}>{t.replaceAll("_", " ")}</option>)}
      </select></label>
      {panel.slots.map((value, s) => <fieldset key={s}><legend>Slot {s + 1} · {templates[panel.template][s]}</legend>
        <label>Source for {panel.name} slot {s + 1} <select value={value?.source ?? "empty"} onChange={(e) => slot(p, s, e.target.value === "empty" ? null : { ...(value ?? widget(templates[panel.template][s])), source: e.target.value as Source })}>
          <option value="empty">Empty</option>
          {(templates[panel.template][s] === "clock" ? ["clock"] : sources).map((source) => <option key={source} value={source}>{source}</option>)}
        </select></label>
        {value && <><label>Intensity for {panel.name} slot {s + 1} <input type="number" min={1} max={255} placeholder="inherit" value={value.intensity ?? ""}
          onChange={(e) => slot(p, s, { ...value, intensity: e.target.value === "" ? null : Number(e.target.value) })} /></label>
          {value.presentation === "gauge" && <label><input type="checkbox" checked={value.reversed} onChange={(e) => slot(p, s, { ...value, reversed: e.target.checked })} /> Fill downward</label>}
          {value.presentation === "clock" && <><p>24-hour local clock. AM/PM formats are deferred until a legible 9-pixel design is agreed.</p>
            <label><input type="checkbox" checked={value.hide_leading_hour_zero ?? false} onChange={(e) => slot(p, s, { ...value, hide_leading_hour_zero: e.target.checked })} /> Hide leading hour zero</label></>}
        </>}
      </fieldset>)}
      {panels.length === 2 && <button type="button" onClick={() => change({ ...config, panels: panels.filter((_, i) => i !== p) })}>Remove {panel.name}</button>}
    </fieldset>)}
    {panels.length === 1 && <button type="button" onClick={() => change({ ...config, panels: [...panels, { name: panels[0].name === "left" ? "right" : "left", template: "clock_gauge", slots: templates.clock_gauge.map(widget) }] })}>Add second panel</button>}
  </fieldset>;
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
    try { const result = await invoke<Frame[]>("render_draft", { config, sample }); if (current === generation.current) { setFrames(result); setMessage(sample ? "SAMPLE DATA · not live readings" : "Live local time; unavailable sources remain blank"); } }
    catch { if (current === generation.current) { setFrames(null); setMessage("Preview rejected or daemon unavailable. Validate the draft and retry."); } }
    finally { if (current === generation.current) setBusy(false); }
  }
  return <section aria-label="Draft preview">
    <button type="button" disabled={busy} onClick={() => void preview(true)}>Preview with sample data</button>
    <button type="button" disabled={busy} onClick={() => void preview(false)}>Preview with live time</button>
    <p>{message}</p><div className="draft-panels">{frames?.map((frame, i) => <figure key={i}><figcaption>{config.panels[i].name}</figcaption>
      <div className="panel" role="img" aria-label={`Draft preview for ${config.panels[i].name}`}><div className="pixels" aria-hidden="true">{frame.pixels.map((pixel, n) => <span key={n} className="pixel" style={{ opacity: pixel ? pixel / 255 : 0.06 }} />)}</div></div>
    </figure>)}</div>
  </section>;
}

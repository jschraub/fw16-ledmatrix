import { useState, useImperativeHandle, type Ref } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Config } from "./bindings/Config";
import type { DisplaySources } from "./bindings/DisplaySources";
import type { SavedConfig } from "./bindings/SavedConfig";
import type { ConfigReply } from "./bindings/ConfigReply";
import type { Request } from "./bindings/Request";
import { Layouts, DraftPreview } from "./Layouts";
import { Bindings } from "./Bindings";
import { BrightnessEditor } from "./BrightnessEditor";
import { HardwarePreview } from "./HardwarePreview";
import { ViewNav } from "./ViewNav";

const tasks = ["Layout", "Brightness", "Assignments", "Preview"] as const;

export function ConfigEditor({ panelName = null, ref, sources }: { panelName?: string | null; ref?: Ref<{ loadIfNeeded(): void }>; sources?: DisplaySources }) {
  const [task, setTask] = useState<typeof tasks[number]>("Layout");
  const [saved, setSaved] = useState<SavedConfig | null>(null);
  const [draft, setDraft] = useState<Config | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [conflict, setConflict] = useState<SavedConfig | null>(null);
  function encodable(value: unknown): boolean {
    if (typeof value === "number") return Number.isInteger(value) && value >= 0 && value <= 255;
    if (Array.isArray(value)) return value.every(encodable);
    if (value && typeof value === "object") return Object.values(value).every(encodable);
    return true;
  }
  const numericSettingsValid = draft === null || encodable(draft);
  async function command(request: Request, retainDraft = false) {
    setBusy(true);
    try {
      const reply = await invoke<ConfigReply>("configuration", { request });
      setMessage(reply.errors.length ? reply.errors.map((e) => `${e.field}: ${e.message}`).join("; ") : reply.status);
      if (reply.status === "conflict") setConflict(reply.saved);
      if (reply.status === "read" || reply.status === "applied") {
        setSaved(reply.saved); setConflict(null);
        if (!retainDraft) setDraft(reply.saved.config);
      }
    } catch { setMessage("Configuration unavailable. Reconnect and retry; your draft is retained."); }
    finally { setBusy(false); }
  }
  useImperativeHandle(ref, () => ({ loadIfNeeded() { if (!draft && !busy) void command({ type: "read_config" }); } }));
  const panel = draft?.panels?.find(p => p.name === panelName);
  return <section aria-label="Configuration">
    <h2>{panelName ? `Customize ${panelName} panel` : "Configuration"}</h2>
    <button disabled={busy} onClick={() => void command({ type: "read_config" })}>Load saved configuration</button>
    {draft && saved && <>
      <p>Saved revision {saved.revision} · {JSON.stringify(draft) === JSON.stringify(saved.config) ? "No draft changes" : "Unsaved draft"}</p>
      <details open={conflict ? true : undefined}><summary>Revision options</summary>
      <button disabled={busy} onClick={() => void command({ type: "read_config" }, true)}>Refresh saved state (keep draft)</button>
      </details>
      {conflict && <div role="alert">
        Another client saved revision {conflict.revision}. Your draft and original base revision are unchanged.
        <p>Current saved clock intensity: {conflict.config.clock_intensity}. Draft clock intensity: {draft.clock_intensity}.</p>
        <p>Refresh to adopt the current revision while keeping your draft, review it, then Apply explicitly; or load saved configuration to discard your draft.</p>
      </div>}
      {panelName ? panel && <label>Panel brightness <input type="number" min={1} max={255}
        value={(panel.brightness_override ?? draft.brightness).manual}
        onChange={e => setDraft({ ...draft, panels: draft.panels.map(p => p !== panel ? p : {
          ...p, brightness_override: { ...(p.brightness_override ?? draft.brightness), manual: Number(e.target.value) },
        }) })} /></label> : null}
      {!panelName && <ViewNav label="Configuration tasks" items={tasks} current={task} select={setTask} />}
      <div hidden={!panelName && task !== "Layout"}>{!panelName && <label>Clock intensity <input type="number" min={1} max={255} value={draft.clock_intensity}
        onChange={(e) => setDraft({ ...draft, clock_intensity: Number(e.target.value) })} /></label>}
      <Layouts config={draft} change={setDraft} panelName={panelName} /></div>
      <div hidden={!!panelName}>
      <div hidden={task !== "Brightness"}><BrightnessEditor config={draft} change={setDraft} sources={sources} /></div>
      <div hidden={task !== "Assignments"}><Bindings config={draft} change={setDraft} /></div>
      <div hidden={task !== "Preview"}>
      {draft.panels && <DraftPreview config={draft} />}
      </div></div>
      <div hidden={!panelName && task !== "Preview"}><HardwarePreview config={draft} /></div>
      <div className="draft-actions">
      {panelName && <p>Preview, Apply, and Cancel draft include all unsaved panel changes.</p>}
      {!numericSettingsValid && <p role="alert">Numeric settings must be whole values in the 0–255 wire range; use the narrower limits shown by each control.</p>}
      <button disabled={busy || !numericSettingsValid} onClick={() => void command({ type: "validate_config", config: draft })}>Validate</button>
      <button disabled={busy || !numericSettingsValid || draft.clock_intensity < 1}
        onClick={() => void command({ type: "apply_config", expected_revision: saved.revision, config: draft })}>Apply</button>
      <button disabled={busy} onClick={() => { setDraft(saved.config); setMessage("Draft cancelled"); }}>Cancel draft</button>
      </div>
    </>}
    <p aria-live="polite">{message}</p>
  </section>;
}

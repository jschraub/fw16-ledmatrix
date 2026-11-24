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
import { FeedbackEditor } from "./FeedbackEditor";
import { ViewNav } from "./ViewNav";
import { BayEditor } from "./BayEditor";
import { WorkspaceTitle } from "./WorkspaceTitle";
import { useActionNotice, type Notice } from "./useActionNotice";

const tasks = ["Layout", "Brightness", "Feedback", "Assignments", "Preview"] as const;
const outcomes = {
  read: {tone:"neutral", text:"Settings loaded."},
  valid: {tone:"success", text:"Settings valid."},
  applied: {tone:"success", text:"Changes saved."},
  invalid: {tone:"error", text:"Settings are invalid."},
  conflict: {tone:"error", text:"Saved settings have changed."},
  save_failed: {tone:"error", text:"Couldn’t save. Your draft is still here."},
} satisfies Record<ConfigReply["status"], Notice>;

export interface EditorActions { loadIfNeeded(): void; openAssignments(): void }
export function ConfigEditor({ panelName = null, ref, sources }: { panelName?: string | null; ref?: Ref<EditorActions>; sources?: DisplaySources }) {
  const [task, setTask] = useState<typeof tasks[number]>("Layout");
  const [saved, setSaved] = useState<SavedConfig | null>(null);
  const [draft, setDraft] = useState<Config | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useActionNotice();
  const [conflict, setConflict] = useState<SavedConfig | null>(null);
  function encodable(value: unknown): boolean {
    if (typeof value === "number") return Number.isInteger(value) && value >= 0 && value <= 255;
    if (Array.isArray(value)) return value.every(encodable);
    if (value && typeof value === "object") return Object.values(value).every(encodable);
    return true;
  }
  const numericSettingsValid = draft === null || encodable(draft);
  async function command(request: Request, retainDraft = false, automatic = false) {
    setBusy(true);
    setNotice(null);
    try {
      const reply = await invoke<ConfigReply>("configuration", { request });
      if (reply.errors.length) setNotice({tone:"error", text:reply.errors.map(e => `${e.field}: ${e.message}`).join("; ")});
      else if (!automatic || reply.status !== "read") setNotice({...outcomes[reply.status]});
      if (reply.status === "conflict") setConflict(reply.saved);
      if (reply.status === "read" || reply.status === "applied") {
        setSaved(reply.saved); setConflict(null);
        if (!retainDraft) setDraft(reply.saved.config);
      }
    } catch { setNotice({tone:"error", text:"Reconnect and retry. Your draft is still here."}); }
    finally { setBusy(false); }
  }
  function loadIfNeeded() { if (!draft && !busy) void command({ type: "read_config" }, false, true); }
  useImperativeHandle(ref, () => ({ loadIfNeeded, openAssignments() { setTask("Assignments"); loadIfNeeded(); } }));
  const panel = draft?.panels?.find(p => p.name === panelName);
  return <section aria-label="Configuration">
    {panelName && <p className="kicker">Panel editor</p>}
    {panelName ? <h2 className="bay-name">{panelName} panel</h2> : <WorkspaceTitle title="Configuration" purpose="Settings"/>}
    {!draft && <button disabled={busy} onClick={() => void command({ type: "read_config" })}>{busy ? "Loading…" : "Load saved configuration"}</button>}
    {draft && saved && <>
      <p className="draft-state">{JSON.stringify(draft) === JSON.stringify(saved.config) ? "Saved settings" : "Unsaved draft"}</p>
      {conflict && <div role="alert">
        Another client saved revision {conflict.revision}. Your draft and original base revision are unchanged.
        <p>Current saved clock intensity: {conflict.config.clock_intensity}. Draft clock intensity: {draft.clock_intensity}.</p>
        <p>Refresh to keep your draft, then save. Reload to use the saved settings instead.</p>
      </div>}
      {panel && <BayEditor config={draft} panel={panel} change={setDraft}/>}
      {!panelName && <ViewNav label="Configuration tasks" items={tasks} current={task} select={setTask} />}
      <div hidden={!panelName && task !== "Layout"}>{!panelName && <>
        <Layouts config={draft} change={setDraft} />
        <label className="global-field">Clock intensity <input type="number" min={1} max={255} value={draft.clock_intensity}
          onChange={(e) => setDraft({ ...draft, clock_intensity: Number(e.target.value) })} /></label></>}</div>
      <div hidden={!!panelName}>
      <div hidden={task !== "Brightness"}><BrightnessEditor config={draft} change={setDraft} sources={sources} /></div>
      <div hidden={task !== "Feedback"}><FeedbackEditor config={draft} change={setDraft} /></div>
      <div hidden={task !== "Assignments"}><Bindings config={draft} change={setDraft} /></div>
      <div hidden={task !== "Preview"}>
      {draft.panels && <DraftPreview config={draft} />}
      </div></div>
      {!panelName && <div hidden={task !== "Preview"}><HardwarePreview config={draft} /></div>}
      <div className="draft-actions">
      {!numericSettingsValid && <p role="alert">Numeric settings must be whole values in the 0–255 wire range; use the narrower limits shown by each control.</p>}
      <button disabled={busy || !numericSettingsValid || draft.clock_intensity < 1}
        onClick={() => void command({ type: "apply_config", expected_revision: saved.revision, config: draft })}>Save changes</button>
      <button disabled={busy} onClick={() => { setDraft(saved.config); setNotice({tone:"neutral", text:"Changes discarded."}); }}>Discard changes</button>
      {!panelName && <button disabled={busy || !numericSettingsValid} onClick={() => void command({ type: "validate_config", config: draft })}>Validate</button>}
      </div>
      <details hidden={!!panelName && !conflict} open={conflict ? true : undefined}><summary>Revision options</summary>
      <p>Saved revision {saved.revision}</p>
      <button disabled={busy} onClick={() => void command({ type: "read_config" }, true)}>Refresh saved state (keep draft)</button>
      <button disabled={busy} onClick={() => void command({type:"read_config"})}>Reload and discard draft</button>
      </details>
    </>}
    <div className="action-notice" data-tone={notice?.tone} aria-live="polite" aria-atomic="true">{notice?.text}</div>
  </section>;
}

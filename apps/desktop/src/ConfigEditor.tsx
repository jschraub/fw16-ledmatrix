import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Config } from "./bindings/Config";
import type { SavedConfig } from "./bindings/SavedConfig";
import type { ConfigReply } from "./bindings/ConfigReply";
import type { Request } from "./bindings/Request";
import { Layouts, DraftPreview } from "./Layouts";

export function ConfigEditor() {
  const [saved, setSaved] = useState<SavedConfig | null>(null);
  const [draft, setDraft] = useState<Config | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [conflict, setConflict] = useState<SavedConfig | null>(null);
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
  return <section aria-label="Configuration">
    <h2>Configuration</h2>
    <button disabled={busy} onClick={() => void command({ type: "read_config" })}>Load saved configuration</button>
    {draft && saved && <>
      <p>Saved revision {saved.revision}. Changes remain a local draft until Apply.</p>
      <p>Saved configuration is desired state, not proof that a disconnected physical panel has rendered it.</p>
      <button disabled={busy} onClick={() => void command({ type: "read_config" }, true)}>Refresh saved state (keep draft)</button>
      {conflict && <div role="alert">
        Another client saved revision {conflict.revision}. Your draft and original base revision are unchanged.
        <p>Current saved clock intensity: {conflict.config.clock_intensity}. Draft clock intensity: {draft.clock_intensity}.</p>
        <p>Refresh to adopt the current revision while keeping your draft, review it, then Apply explicitly; or load saved configuration to discard your draft.</p>
      </div>}
      <label>Clock intensity <input type="number" min={1} max={255} value={draft.clock_intensity}
        onChange={(e) => setDraft({ ...draft, clock_intensity: Number(e.target.value) })} /></label>
      <Layouts config={draft} change={setDraft} />
      {draft.panels && <DraftPreview config={draft} />}
      <button disabled={busy} onClick={() => void command({ type: "validate_config", config: draft })}>Validate</button>
      <button disabled={busy || !Number.isInteger(draft.clock_intensity) || draft.clock_intensity < 1 || draft.clock_intensity > 255}
        onClick={() => void command({ type: "apply_config", expected_revision: saved.revision, config: draft })}>Apply</button>
      <button disabled={busy} onClick={() => { setDraft(saved.config); setMessage("Draft cancelled"); }}>Cancel draft</button>
    </>}
    <p aria-live="polite">{message}</p>
  </section>;
}

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Config } from "./bindings/Config";
import type { PreviewReply } from "./bindings/PreviewReply";

export function HardwarePreview({ config }: { config: Config }) {
  const owner = useRef<string | null>(null);
  const [active, setActive] = useState(false);
  const [message, setMessage] = useState("");
  useEffect(() => () => { if (owner.current) void invoke("hardware_preview", { request: { type: "cancel_preview", owner: owner.current } }).catch(() => {}); }, []);
  async function begin() {
    const token = crypto.randomUUID(); owner.current = token; setActive(true); setMessage("Temporary output preview starting; the original draft is used for this lease.");
    try {
      const reply = await invoke<PreviewReply>("hardware_preview", { request: { type: "begin_preview", owner: token, config, seconds: 10 } });
      setMessage(reply.status === "busy" ? "Another client owns the preview. Your draft is unchanged." : `Preview ${reply.status}; latest applied state restored when this lease ends.`);
    } catch { setMessage("Preview connection failed. The daemon restores applied output on disconnect or expiry."); }
    finally { if (owner.current === token) owner.current = null; setActive(false); }
  }
  async function cancel() {
    if (!owner.current) return;
    try { await invoke("hardware_preview", { request: { type: "cancel_preview", owner: owner.current } }); setMessage("Cancellation requested; restoring latest applied output."); }
    catch { setMessage("Cancellation could not be delivered. The bounded lease still expires automatically."); }
  }
  return <section aria-label="Temporary hardware preview">
    <p>Only one preview owner at a time. Identification has higher priority. Apply changes saved state but does not silently end the preview; expiry/cancellation restores the latest applied state, not the original draft. Transport settling may delay physical restoration.</p>
    <button type="button" disabled={active || !config.panels?.some((panel) => panel.binding)} onClick={() => void begin()}>Preview on assigned hardware (10 seconds)</button>
    <button type="button" disabled={!active} onClick={() => void cancel()}>Cancel hardware preview</button>
    <p aria-live="polite">{message}</p>
  </section>;
}

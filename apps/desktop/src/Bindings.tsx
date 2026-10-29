import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DeviceReply } from "./bindings/DeviceReply";
import type { Config } from "./bindings/Config";

export function Bindings({ config, change }: { config: Config; change: (config: Config) => void }) {
  const [reply, setReply] = useState<DeviceReply | null>(null);
  const [error, setError] = useState(false);
  async function refresh() {
    setError(false);
    try { setReply(await invoke<DeviceReply>("devices", { request: { type: "devices" } })); }
    catch { setError(true); }
  }
  return <fieldset><legend>Physical assignment</legend>
    <p>Identify the panel in Devices first. Assign by stable physical location, never serial number or port order. An unknown/ambiguous location cannot be assigned safely. Save with Apply.</p>
    <button onClick={() => void refresh()}>Refresh assignment candidates</button>
    {error && <p role="alert">Cannot list devices. Your assignments and draft are unchanged.</p>}
    {(config.panels ?? []).map((panel, index) => <label key={panel.name}>Hardware for {panel.name} <select value={panel.binding ?? ""} onChange={(e) => change({ ...config, panels: config.panels.map((p, i) => i === index ? { ...p, binding: e.target.value || null } : p) })}>
      <option value="">Unassigned (simulated)</option>
      {panel.binding && !reply?.devices.some((d) => d.location === panel.binding) && <option value={panel.binding}>Assigned location currently disconnected / not refreshed</option>}
      {reply?.devices.map((device) => <option key={device.port} value={device.location ?? `unknown:${device.port}`} disabled={!device.location || reply.devices.filter((other) => other.location === device.location).length !== 1}>{device.port} · {device.location ?? "unknown location; reassignment support pending"}</option>)}
    </select></label>)}
  </fieldset>;
}

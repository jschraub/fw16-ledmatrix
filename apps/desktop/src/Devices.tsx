import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DeviceReply } from "./bindings/DeviceReply";
import type { Request } from "./bindings/Request";

export function Devices() {
  const [reply, setReply] = useState<DeviceReply | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const [error, setError] = useState("");
  async function command(request: Request) {
    setError("");
    if (request.type === "identify") setActive(request.port);
    try { setReply(await invoke<DeviceReply>("devices", { request })); }
    catch { setError("Device operation failed. Check the daemon connection, access permissions and competing POC processes."); }
    finally { if (request.type === "identify") setActive(null); }
  }
  return <section aria-label="Physical devices"><h2>Devices</h2>
    <p>Identification explicitly opens a Framework device, queries firmware, outlines it for at most two seconds after wake, then restores the current normal clock frame. Role-based output follows assignment; never run the POC concurrently.</p>
    <button onClick={() => void command({ type: "devices" })}>Discover physical panels</button>
    {error && <p role="alert">{error}</p>}
    {reply && <><p>{reply.message}</p>{reply.devices.length === 0 && <p>No physical panels detected. Simulated development is still available.</p>}
      {reply.devices.map((device) => <fieldset key={device.port}><legend>{device.port}</legend>
        <p>{device.status} · Firmware {device.firmware ?? "not queried"}</p>
        <p>Location: {device.location ?? "unknown; do not infer bay from COM/serial order"}</p>
        <button disabled={active !== null} onClick={() => void command({ type: "identify", port: device.port })}>{active === device.port ? "Identifying…" : "Identify panel"}</button>
        {active === device.port && <button onClick={() => void command({ type: "cancel_identify", port: device.port })}>Cancel identification</button>}
      </fieldset>)}
    </>}
  </section>;
}

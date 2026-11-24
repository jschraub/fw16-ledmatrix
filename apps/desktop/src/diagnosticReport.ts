import type { Snapshot } from "./bindings/Snapshot";
import type { Device } from "./bindings/Device";
import failures from "./bindings/failures.json";

// Map raw input to a closed vocabulary; never serialize the original snapshot,
// config, exception, probe detail or device (all can contain identifying text).
function known(value: unknown, allowed: string[]) {
  return typeof value === "string" && allowed.includes(value) ? value : "unknown";
}
function state(device: Device) {
  if (device.status.startsWith("device write failed") || device.status.startsWith("sleep command failed")) return "write failed";
  if (device.status.startsWith("reconnecting:")) return "retrying";
  if (device.status.startsWith("open;")) return "open";
  if (device.status.startsWith("sleeping ")) return "sleeping";
  if (device.status.startsWith("busy:")) return "busy";
  if (device.status.startsWith("detected;")) return "detected";
  return "unknown";
}
export function diagnosticReport(snapshot: Snapshot | null, failure: string | undefined, devices: Device[] | null) {
  return JSON.stringify({
    format: 1,
    connection: snapshot ? "connected" : failure ? known(failure, Object.keys(failures)) : "no snapshot",
    deviceScan: devices === null ? "unavailable" : devices.length,
    panels: snapshot?.panels.map(p => {
      const matches = devices?.filter(d => p.binding !== null && d.location === p.binding);
      return { role: known(p.name, ["left", "right"]), state: !p.binding ? "unassigned" :
        !matches ? "unknown" : !matches.length ? "missing" : matches.length !== 1 ? "ambiguous" : state(matches[0]) };
    }) ?? [],
    battery: known(snapshot?.power?.status, ["available", "unavailable", "error"]),
    display: snapshot?.display_sources ? {
      connectors: snapshot.display_sources.connected.length,
      readableBacklights: Object.values(snapshot.display_sources.backlights).filter(v => v !== null).length,
    } : null,
    sources: ["claude", "opencode_openai"].map(provider => ({
      provider,
      session: known(snapshot?.assistants.health.find(h => h.provider === provider)?.status,
        ["live", "missing", "stale", "error", "error_or_limited"]),
      usage: known(snapshot?.assistants.usage?.find(u => u.provider === provider)?.status,
        ["disabled", "unsupported_login", "unauthorized", "account_changed", "fresh", "no_windows", "schema_error", "request_failed", "expired"]),
    })),
  }, null, 2);
}

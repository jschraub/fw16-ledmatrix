import type { AssistantState } from "./bindings/AssistantState";
import type { Window as UsageWindow } from "./bindings/Window";
function reading(w: UsageWindow | null) {
  return w ? `${w.used_percent}% used${w.resets_at === null ? "" : ` · resets ${new Date(w.resets_at * 1000).toLocaleString()}`}` : "unavailable";
}
export function Integrations({ state }: { state: AssistantState }) {
  return <section aria-label="Assistant integrations"><h2>Assistant integrations</h2>
    <p>Metadata only: no conversation text or credentials. Follow latest active main session, or paste a session ID into a widget's explicit pin. This follows activity, not terminal focus. Quota adapters are independent of session feeds.</p>
    <p>Producer setup instructions are in docs/INTEGRATIONS.md. Missing producers leave their slots blank rather than showing zero.</p>
    <h3>Account usage</h3>
    {(state.usage ?? []).map(u => <section key={u.provider}>
      <h4>{u.provider === "claude" ? "Claude" : "OpenAI"}</h4>
      <p>{u.status.replaceAll("_", " ")}{u.status === "disabled" ? " · polling is off" : ""}{u.expires_at !== null && ["schema_error", "request_failed"].includes(u.status) ? " · cached, stale" : ""}</p>
      <p>Five hours: {reading(u.five_hour)} · Weekly: {reading(u.weekly)}</p>
      {u.expires_at !== null && <small>Cache expires {new Date(u.expires_at * 1000).toLocaleString()}</small>}
    </section>)}
    <h3>Session feeds</h3>
    {state.health.map((health) => <p key={health.provider}>{health.provider}: {health.status} · {health.detail}</p>)}
    <ul>{state.sessions.map((session) => <li key={`${session.provider}:${session.session_id}`}>
      {session.provider} · {session.session_id} · context {session.context_percent ?? "unavailable"} · {session.busy ? "working" : "idle"}
      <br />Last activity: {new Date(session.last_activity_ms).toISOString()} · heartbeat: {new Date(session.heartbeat_ms).toISOString()}
    </li>)}</ul>
  </section>;
}

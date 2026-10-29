import type { AssistantState } from "./bindings/AssistantState";
export function Integrations({ state }: { state: AssistantState }) {
  return <section aria-label="Assistant integrations"><h2>Assistant integrations</h2>
    <p>Metadata only: no conversation text or credentials. Follow latest active main session, or paste a session ID into a widget's explicit pin. This follows activity, not terminal focus. Quota adapters are independent of session feeds.</p>
    <p>Producer setup instructions are in docs/INTEGRATIONS.md. Missing producers leave their slots blank rather than showing zero.</p>
    {state.health.map((health) => <p key={health.provider}>{health.provider}: {health.status} · {health.detail}</p>)}
    <ul>{state.sessions.map((session) => <li key={`${session.provider}:${session.session_id}`}>
      {session.provider} · {session.session_id} · context {session.context_percent ?? "unavailable"} · {session.busy ? "working" : "idle"}
      <br />Last activity: {new Date(session.last_activity_ms).toISOString()} · heartbeat: {new Date(session.heartbeat_ms).toISOString()}
    </li>)}</ul>
  </section>;
}

import type { AssistantState } from "./bindings/AssistantState";
import type { UsageStatus } from "./bindings/UsageStatus";
import type { Window as UsageWindow } from "./bindings/Window";

const labels:Record<UsageStatus,string> = {
  disabled:"Polling off",unsupported_login:"Login unavailable",unauthorized:"Sign-in required",
  account_changed:"Account changed",fresh:"Current",no_windows:"No windows reported",
  schema_error:"Response changed",request_failed:"Request failed",expired:"Expired",
};
function Allowance({name,window}:{name:string;window?:UsageWindow|null}) {
  return <div><dt>{name}</dt><dd>{window?`${window.used_percent}% used`:"Unavailable"}
    {window?.resets_at!=null && <small>Resets {new Date(window.resets_at*1000).toLocaleString()}</small>}</dd></div>;
}
export function Integrations({state}:{state:AssistantState}) {
  return <section aria-label="Assistant integrations"><div className="provider-grid">
    {(["opencode_openai","claude"] as const).map(provider=>{
      const usage=state.usage?.find(u=>u.provider===provider);
      const health=state.health.find(h=>h.provider===provider);
      const sessions=state.sessions.filter(s=>s.provider===provider);
      const stale=usage?.expires_at!=null && ["schema_error","request_failed"].includes(usage.status);
      return <article className="provider-sheet" key={provider}>
        <h3>{provider==="claude"?"Claude":"OpenAI"}</h3>
        <div className="section-line"><h4>Account usage</h4><small className="state-label" data-tone={stale?"error":usage?.status==="fresh"?"success":undefined}>
          {usage?labels[usage.status]:"Not reported"}{stale?" · cached, stale":""}</small></div>
        <dl className="allowances"><Allowance name="Five hours" window={usage?.five_hour}/><Allowance name="Weekly" window={usage?.weekly}/></dl>
        <div className="section-line"><h4>Session feed</h4><small className="state-label" data-tone={health?.status==="live"?"success":undefined}>
          {health?.status==="live"?"Live":health?.status==="missing"?"No feed":health?.status.replaceAll("_"," ")??"Not reported"}</small></div>
        {sessions.length>0 ? <details><summary>{sessions.length} {sessions.length===1?"session":"sessions"}</summary>
          <ul className="session-list">{sessions.map(s=><li key={`${s.instance_id}:${s.session_id}`}>
            <code>{s.session_id}</code><p>Context {s.context_percent==null?"unavailable":`${s.context_percent}%`} · {s.busy?"Working":"Idle"}</p>
            <small>Last activity: {new Date(s.last_activity_ms).toISOString()} · Heartbeat: {new Date(s.heartbeat_ms).toISOString()}</small>
          </li>)}</ul></details> : <p className="muted">No active sessions reported.</p>}
        <details><summary>Source details</summary>
          {usage?.expires_at!=null && <p>Usage cache expires {new Date(usage.expires_at*1000).toLocaleString()}.</p>}
          <p>{health?.detail||"No session feed details available."}</p>
        </details>
      </article>;
    })}
  </div><details className="workspace-bottom"><summary>Setup and session selection</summary>
    <p>Account polling and session feeds work independently. A widget follows the latest active main session unless you pin a session ID in Configuration.</p>
    <p>Setup: <code>docs/INTEGRATIONS.md</code> in the checkout or <code>/usr/share/doc/fw16-ledmatrix/INTEGRATIONS.md</code> in the Linux package.</p>
  </details></section>;
}

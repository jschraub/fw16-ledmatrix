import type { Snapshot } from "./bindings/Snapshot";
import type { Frame } from "./bindings/Frame";
import type { Source } from "./bindings/Source";

const labels: Record<Source,string> = {clock:"Clock", battery:"Battery", context:"Context", activity:"Activity", five_hour:"Five-hour usage", weekly:"Weekly usage"};
export function PanelDisplay({frame,label}:{frame: Frame; label:string}) {
  return <span className="panel" role="img" aria-label={label}><span className="pixels" aria-hidden="true">
    {frame.pixels.map((v,i) => <span key={i} className="pixel" style={{opacity:v ? v/255 : 0.06}} />)}
  </span></span>;
}
export function PanelMap({snapshot, connected, edit, setup}:{snapshot:Snapshot|null; connected:boolean; edit:(name:string)=>void; setup:()=>void}) {
  const panels = [...(snapshot?.panels ?? [])].sort((a,b)=>a.name.localeCompare(b.name));
  return <section className="panel-map" aria-labelledby="map-title">
    <p className="kicker">Signal room</p>
    <h2 id="map-title" tabIndex={-1}>Your panels<span aria-hidden="true">.</span></h2>
    {!snapshot ? <p>{connected ? "Waiting for panel output…" : "Connect to see your panels and their current output."}</p> :
      <><p>{snapshot.preview ? "Temporary preview · saved settings haven’t changed." : "Live output · select a panel to edit its layout and brightness."}</p>
      {panels.length ? <div className="map-grid">{panels.map((p,i) =>
        <button key={p.name} className="map-card" aria-label={`Customize ${p.name} panel`} onClick={()=>edit(p.name)}>
          <span className="card-title">0{i+1} / {p.name} bay</span>
          <span className="matrix-stage"><PanelDisplay frame={p.frame} label={`Live logical panel ${p.name}`} /></span>
          <span className="caption"><span><small>{p.binding ? "Assigned" : "Unassigned"} · {p.brightness == null ? "Sleeping" : `Level ${p.brightness}`}</small>
          <strong>{p.sources ? p.sources.map(s=>labels[s]).join(" + ") || "Empty layout" : "Panel output"}</strong></span><span aria-hidden="true">↗</span></span>
        </button>)}</div> :
        <div><p>Simulated output</p><PanelDisplay frame={snapshot.frame} label={`Simulated LED panel showing ${String(snapshot.hour).padStart(2,"0")}:${String(snapshot.minute).padStart(2,"0")}`} /></div>}
      <div className="setup-row"><span>{panels.some(p=>p.binding) ? "Need to check which panel is which?" : "Ready to connect a physical panel?"}</span>
        <button onClick={setup}>Find and identify panels</button></div></>}
  </section>;
}

// Throwaway #41: should changing a panel start with its bay, a controls list, or a task?
import { useEffect, useState } from "react";
import "./design-prototype.css";

const names = ["Panel map", "Inspector", "Guided change"];
const sides = ["Left", "Right"] as const;
type Side = typeof sides[number];
const role = { Left: "Battery", Right: "Clock" };
export function DesignPrototype() {
  const initial = "ABC".indexOf(new URLSearchParams(location.search).get("variant") || "A");
  const [design, setDesign] = useState(Math.max(0, initial));
  const [side, select] = useState<Side>("Right");
  const [saved, save] = useState({ Left: 32, Right: 32 });
  const [draft, edit] = useState(saved);
  const [preview, show] = useState<typeof saved | null>(null);
  const [step, advance] = useState(0);
  const [editing, open] = useState(false);
  function cycle(delta: number) {
    const n = (design + 3 + delta) % 3;
    history.replaceState(null, "", `?variant=${"ABC"[n]}`);
    setDesign(n); advance(0); open(false);
  }
  useEffect(() => {
    function key(e: KeyboardEvent) {
      if ((e.target as HTMLElement).closest("input,select,textarea,[contenteditable]")) return;
      if (["ArrowLeft","ArrowRight"].includes(e.key)) { e.preventDefault(); cycle(e.key === "ArrowLeft" ? -1 : 1); }
    }
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [design]);
  useEffect(() => {
    if (!preview) return;
    const id = setTimeout(() => show(null), 10000);
    return () => clearTimeout(id);
  }, [preview]);
  const changed = sides.some(s => draft[s] !== saved[s]);
  const controls = <label>{side} panel brightness <input type="number" min="1" max="255" value={draft[side]}
    onChange={e => edit({ ...draft, [side]: Math.min(255, Math.max(1, Number(e.target.value))) })} /></label>;
  const actions = <div className="actions"><button disabled={!changed} onClick={() => show({...draft})}>Try for 10 seconds</button>
    <button disabled={!changed} onClick={() => { save(draft); show(null); open(false); advance(0); }}>Save changes</button>
    <button disabled={!changed} onClick={() => { edit(saved); show(null); open(false); advance(0); }}>Discard draft</button></div>;
  function pick(s: Side) { select(s); open(true); }
  function tile(s: Side) {
    return <button key={s} className="bay" aria-pressed={s === side} onClick={() => pick(s)}>
      <small>{s.toUpperCase()} PANEL</small><strong>{role[s]}</strong>
      <span className="readout">{s === "Left" ? "▮▮▮▮▯" : "12:34"}</span>
      <span>Brightness {(preview ?? saved)[s]}</span></button>;
  }
  return <main className="prototype"><header><p className="eyebrow">FRAMEWORK LAPTOP 16 · DESIGN PROTOTYPE</p>
    <h1>LED Matrix</h1><p>Sample panels · Changes last only in this browser tab.</p></header>
    {design === 0 ? <><h2>Your panels</h2><p>Select a panel to customize it.</p>
      <div className="map">{sides.map(tile)}</div>{editing && <section><h2>{side} panel</h2>{controls}{actions}</section>}</>
    : design === 1 ? <div className="inspector"><aside><h2>Panels</h2>{sides.map(s => <button key={s} aria-pressed={s === side} onClick={() => pick(s)}>{s} · {role[s]}</button>)}</aside>
      <section><small>PANEL INSPECTOR</small><h2>{side} · {role[side]}</h2><p>Current brightness: {(preview ?? saved)[side]}</p>{controls}{actions}</section></div>
    : <section><small>STEP {step + 1} OF 3</small><h2>{["Which panel?", "Adjust brightness", "Review your change"][step]}</h2>
      {step === 0 ? <div className="map">{sides.map(s => <button key={s} onClick={() => { pick(s); advance(1); }}>{s} · {role[s]} →</button>)}</div>
      : step === 1 ? <>{controls}<button onClick={() => advance(2)}>Review →</button></>
      : <><p>{side} · {role[side]}: {saved[side]} → {draft[side]}</p>{actions}</>}
      {step > 0 && <button onClick={() => advance(step - 1)}>Back</button>}</section>}
    <footer aria-live="polite">{preview ? "Temporary preview" : changed ? "Unsaved draft" : "Saved settings"} · {sides.map(s => `${s}: saved ${saved[s]}, draft ${draft[s]}${preview ? `, preview ${preview[s]}` : ""}`).join(" · ")}</footer>
    <nav className="switcher" aria-label="Compare designs"><button onClick={() => cycle(-1)} aria-label="Previous design">←</button>
      <strong>{"ABC"[design]} · {names[design]}</strong><button onClick={() => cycle(1)} aria-label="Next design">→</button></nav>
  </main>;
}

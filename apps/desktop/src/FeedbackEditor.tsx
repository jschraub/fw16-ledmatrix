import type { Feedback } from "./bindings/Feedback";
import type { Config } from "./bindings/Config";
import type { Pulse } from "./bindings/Pulse";
import type { PanelLayout } from "./bindings/PanelLayout";

const defaults: Feedback = { volume: 0, brightness: 0, ac: 0, volume_width: 5, brightness_width: 5, ac_width: 5, charging: false };
const gauges = { volume: "Volume and mute", brightness: "Brightness change", ac: "Battery level on AC change" };

export function FeedbackEditor({ config, change }: { config: Config; change: (c: Config) => void }) {
  return <section aria-label="Temporary feedback"><h3>Temporary feedback</h3>
    <p>Choose which panel shows each gauge, and for how long. New changes replace the gauge already on that panel.</p>
    {config.panels?.map((panel, i) => {
      return <fieldset key={panel.name}><legend>{panel.name} panel</legend>
        <PanelEffects panel={panel} change={feedback=>change({...config,panels:config.panels.map((p,n)=>n===i?{...p,feedback}:p)})}/>
      </fieldset>;
    })}
  </section>;
}

export function PanelEffects({panel,change}:{panel:PanelLayout;change:(feedback:Feedback)=>void}) {
  const settings = panel.feedback ?? defaults;
  const pulse = settings.pulse ?? { mode: "default" };
  const set = (patch: Partial<Feedback>) => change({...settings,...patch});
  return <>
        <label><input type="checkbox" checked={settings.charging} onChange={e => set({ charging: e.target.checked })} />
          Pulse whole {panel.name} panel while charging</label>
        {settings.charging && <>
          <label>Pulse strength on {panel.name} <select value={pulse.mode} onChange={e => {
            const mode = e.target.value as Pulse["mode"];
            set({pulse: mode === "custom" ? { mode, spread: 32, cycle_seconds: 8 } : { mode }});
          }}>
            <option value="subtle">Subtle</option><option value="default">Default</option>
            <option value="extreme">Extreme</option><option value="custom">Custom</option>
          </select></label>
          {pulse.mode === "custom" && <>
            <label>Brightness spread on {panel.name} (1–254 levels) <input type="number" min={1} max={254} step={1}
              value={pulse.spread || ""} onChange={e => set({pulse:{...pulse, spread:Number(e.target.value)}})} /></label>
            <label>Full cycle on {panel.name} (4–60 seconds) <input type="number" min={4} max={60} step={1}
              value={pulse.cycle_seconds || ""} onChange={e => set({pulse:{...pulse, cycle_seconds:Number(e.target.value)}})} /></label>
            <small>Spread is the difference between dimmest and brightest. One cycle goes up and back down within this panel’s brightness limits.</small>
          </>}
        </>}
    {(Object.keys(gauges) as (keyof typeof gauges)[]).map(k =>
      <div key={k}><label>{gauges[k]} on {panel.name} <select value={settings[k]} onChange={e => set({ [k]: Number(e.target.value) })}>
        <option value={0}>Off</option>
        {Array.from({length:13},(_,i)=>i+3).map(n => <option key={n} value={n}>{n} seconds</option>)}
      </select></label>
      {settings[k] > 0 && <label>{gauges[k]} bar width on {panel.name} <select value={settings[`${k}_width`] ?? 5} onChange={e => set({ [`${k}_width`]: Number(e.target.value) })}>
        {[1, 3, 5, 7, 9].map(width => <option key={width} value={width}>{width} {width === 1 ? "column" : "columns"}</option>)}
      </select></label>}</div>)}
  </>;
}

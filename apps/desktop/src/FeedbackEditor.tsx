import type { Feedback } from "./bindings/Feedback";
import type { Config } from "./bindings/Config";

const defaults: Feedback = { volume: 0, brightness: 0, ac: 0, charging: false };
const gauges = { volume: "Volume and mute", brightness: "Brightness change", ac: "Battery level on AC change" };

export function FeedbackEditor({ config, change }: { config: Config; change: (c: Config) => void }) {
  return <section aria-label="Temporary feedback"><h3>Temporary feedback</h3>
    <p>Choose which panel shows each gauge, and for how long. New changes replace the gauge already on that panel.</p>
    {config.panels?.map((panel, i) => {
      const settings = panel.feedback ?? defaults;
      const set = (patch: Partial<Feedback>) => change({ ...config, panels: config.panels.map((p,n) => n === i ? { ...p, feedback: { ...settings, ...patch } } : p) });
      return <fieldset key={panel.name}><legend>{panel.name} panel</legend>
        {(Object.keys(gauges) as (keyof typeof gauges)[]).map(k =>
          <label key={k}>{gauges[k]} on {panel.name} <select value={settings[k]} onChange={e => set({ [k]: Number(e.target.value) })}>
            <option value={0}>Off</option>
            {Array.from({length:13},(_,i)=>i+3).map(n => <option key={n} value={n}>{n} seconds</option>)}
          </select></label>)}
        <label><input type="checkbox" checked={settings.charging} onChange={e => set({ charging: e.target.checked })} />
          Pulse whole {panel.name} panel while charging</label>
      </fieldset>;
    })}
  </section>;
}

import type { Brightness } from "./bindings/Brightness";
import type { Config } from "./bindings/Config";
import type { DisplaySources } from "./bindings/DisplaySources";
import { DisplayFollow } from "./DisplayFollow";

export const defaultBrightness: Brightness = { minimum: 3, maximum: 255, manual: 32, off: false };
function Controls({ name, value, change }: { name: string; value: Brightness; change: (value: Brightness) => void }) {
  return <fieldset><legend>{name}</legend>
    {(["manual", "minimum", "maximum"] as const).map((field) => <label key={field}>{field==="manual"?"Brightness":field==="minimum"?"Minimum":"Maximum"} <input aria-label={`${name} ${field}`} type="number" min={1} max={255} value={value[field]} onChange={(e) => change({ ...value, [field]: Number(e.target.value) })} /></label>)}
    <label><input aria-label={`${name} off / sleep`} type="checkbox" checked={value.off} onChange={(e) => change({ ...value, off: e.target.checked })} /> Sleep</label>
  </fieldset>;
}
export function BrightnessEditor({ config, change, sources }: { config: Config; change: (config: Config) => void; sources?: DisplaySources }) {
  const global = config.brightness ?? defaultBrightness;
  return <fieldset><legend>Panel brightness</legend>
    <Controls name="Default brightness" value={global} change={(brightness) => change({ ...config, brightness })} />
    <DisplayFollow config={config} change={change} sources={sources} />
    {(config.panels ?? []).map((panel, i) => <div key={panel.name}>
      <label><input type="checkbox" checked={panel.brightness_override != null} onChange={(e) => change({ ...config, panels: config.panels.map((p, n) => n === i ? { ...p, brightness_override: e.target.checked ? { ...global } : null } : p) })} /> Override brightness for {panel.name}</label>
      {panel.brightness_override && <Controls name={`${panel.name} brightness`} value={panel.brightness_override} change={(brightness_override) => change({ ...config, panels: config.panels.map((p, n) => n === i ? { ...p, brightness_override } : p) })} />}
    </div>)}
  </fieldset>;
}

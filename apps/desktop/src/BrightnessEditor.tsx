import type { Brightness } from "./bindings/Brightness";
import type { Config } from "./bindings/Config";

export const defaultBrightness: Brightness = { minimum: 3, maximum: 255, manual: 32, off: false };
function Controls({ name, value, change }: { name: string; value: Brightness; change: (value: Brightness) => void }) {
  return <fieldset><legend>{name}</legend>
    {(["minimum", "maximum", "manual"] as const).map((field) => <label key={field}>{name} {field} <input type="number" min={1} max={255} value={value[field]} onChange={(e) => change({ ...value, [field]: Number(e.target.value) })} /></label>)}
    <label><input type="checkbox" checked={value.off} onChange={(e) => change({ ...value, off: e.target.checked })} /> {name} off / sleep</label>
  </fieldset>;
}
export function BrightnessEditor({ config, change }: { config: Config; change: (config: Config) => void }) {
  const global = config.brightness ?? defaultBrightness;
  return <fieldset><legend>Panel brightness</legend>
    <p>Global panel brightness multiplies widget greyscale; it is not the same as relative intensity. Require minimum ≤ manual ≤ maximum. Explicit off sleeps the device instead of clamping back to the minimum.</p>
    <Controls name="Default brightness" value={global} change={(brightness) => change({ ...config, brightness })} />
    {(config.panels ?? []).map((panel, i) => <div key={panel.name}>
      <label><input type="checkbox" checked={panel.brightness_override != null} onChange={(e) => change({ ...config, panels: config.panels.map((p, n) => n === i ? { ...p, brightness_override: e.target.checked ? { ...global } : null } : p) })} /> Override brightness for {panel.name}</label>
      {panel.brightness_override && <Controls name={`${panel.name} brightness`} value={panel.brightness_override} change={(brightness_override) => change({ ...config, panels: config.panels.map((p, n) => n === i ? { ...p, brightness_override } : p) })} />}
    </div>)}
    <p>Calibrate with the sparse clock and gauge sample preview, not a solid fill. Low limits can make real LEDs illegible. Screen previews cannot establish a physical visibility floor. Overrides are independent of application defaults.</p>
  </fieldset>;
}

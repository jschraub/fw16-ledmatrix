import type { PanelLayout } from "./bindings/PanelLayout";

export function SingleBarAlignment({panel,change}:{panel:PanelLayout;change:(enabled:boolean)=>void}) {
  if (panel.template !== "number_two_gauges" || Boolean(panel.slots[1]) === Boolean(panel.slots[2])) return null;
  return <label><input type="checkbox" checked={panel.center_single_bar ?? false}
    onChange={e=>change(e.target.checked)}/> Center single bar</label>;
}

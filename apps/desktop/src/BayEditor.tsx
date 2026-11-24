import type { PanelLayout } from "./bindings/PanelLayout";
import type { Config } from "./bindings/Config";
import { BayPreview } from "./BayPreview";
import { LayoutComposer } from "./LayoutComposer";
import { HardwarePreview } from "./HardwarePreview";
import { PanelEffects } from "./FeedbackEditor";

export function BayEditor({config,panel,change}:{config:Config; panel:PanelLayout; change:(c:Config)=>void}) {
  const light=panel.brightness_override ?? config.brightness;
  function update(patch:Partial<PanelLayout>) {
    change({...config,panels:config.panels.map(p=>p===panel?{...p,...patch}:p)});
  }
  return <div className="bay-editor">
    <BayPreview key={panel.name} config={config} panel={panel}/>
    <div className="bay-controls">
      <LayoutComposer panel={panel} update={update}/>
      <label>Panel brightness <output>{light.manual}</output><input type="range" min={light.minimum} max={light.maximum} value={light.manual}
        onChange={e=>update({brightness_override:{...light,manual:Number(e.target.value)}})}/></label>
      {panel.binding && <HardwarePreview config={config}/>}
      <details className="panel-effects"><summary>Advanced · Charging pulse &amp; feedback</summary>
        <PanelEffects panel={panel} change={feedback=>update({feedback})}/>
      </details>
    </div>
  </div>;
}

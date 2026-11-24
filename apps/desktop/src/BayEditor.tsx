import type { PanelLayout } from "./bindings/PanelLayout";
import type { Config } from "./bindings/Config";
import type { Frame } from "./bindings/Frame";
import { PanelDisplay } from "./PanelMap";
import { Layouts, DraftPreview } from "./Layouts";
import { HardwarePreview } from "./HardwarePreview";

export function BayEditor({config,panel,output,change}:{config:Config; panel:PanelLayout; output?:Frame; change:(c:Config)=>void}) {
  const light=panel.brightness_override ?? config.brightness;
  function update(patch:Partial<PanelLayout>) {
    change({...config,panels:config.panels.map(p=>p===panel?{...p,...patch}:p)});
  }
  const content=panel.slots.filter(v=>v!==null);
  const only=content.length===1 ? content[0] : undefined;
  const preset=only?.source==="clock" || only?.source==="battery" ? only.source :
    only?.source==="weekly" && only.provider==="opencode_openai" ? "openai-weekly" : "custom";
  return <div className="bay-editor">
    <div className="bay-output">{output && <PanelDisplay frame={output} label={`${panel.name} panel output`} />}<small>{output?"Current output":"Output unavailable"}</small></div>
    <div className="bay-controls">
      <label>Content<select value={preset} onChange={e=>{
        const source=e.target.value==="clock"?"clock":e.target.value==="openai-weekly"?"weekly":"battery";
        const widget={source,presentation:source==="clock"?"clock":"gauge",provider:source==="weekly"?"opencode_openai":"claude",session_id:null,intensity:null,reversed:false,hide_leading_hour_zero:false,warning_threshold:null} as const;
        update(source==="clock"?{template:"clock_gauge",slots:[widget,null]}:{template:"full_gauge",slots:[widget]});
      }}><option value="clock">Clock</option><option value="battery">Battery</option><option value="openai-weekly">OpenAI weekly usage</option><option disabled value="custom">Custom layout</option></select></label>
      <label>Panel brightness <output>{light.manual}</output><input type="range" min={light.minimum} max={light.maximum} value={light.manual}
        onChange={e=>update({brightness_override:{...light,manual:Number(e.target.value)}})}/></label>
      <details><summary>Customize widgets and layout</summary><Layouts config={config} change={change} panelName={panel.name}/></details>
      <details><summary>Preview changes</summary><DraftPreview config={config}/><HardwarePreview config={config}/></details>
    </div>
  </div>;
}

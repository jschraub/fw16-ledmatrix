import type { PanelLayout } from "./bindings/PanelLayout";
import type { Config } from "./bindings/Config";
import type { Frame } from "./bindings/Frame";
import { PanelDisplay } from "./PanelMap";
import { DraftPreview } from "./Layouts";
import { LayoutComposer } from "./LayoutComposer";
import { useState } from "react";
import { HardwarePreview } from "./HardwarePreview";

export function BayEditor({config,panel,output,change}:{config:Config; panel:PanelLayout; output?:Frame; change:(c:Config)=>void}) {
  const light=panel.brightness_override ?? config.brightness;
  function update(patch:Partial<PanelLayout>) {
    change({...config,panels:config.panels.map(p=>p===panel?{...p,...patch}:p)});
  }
  const [preview,setPreview]=useState(false);
  return <div className="bay-editor">
    <div className="bay-output">{output && <PanelDisplay frame={output} label={`${panel.name} panel output`} />}<small>{output?"Current output":"Output unavailable"}</small></div>
    <div className="bay-controls">
      <LayoutComposer panel={panel} update={update}/>
      <label>Panel brightness <output>{light.manual}</output><input type="range" min={light.minimum} max={light.maximum} value={light.manual}
        onChange={e=>update({brightness_override:{...light,manual:Number(e.target.value)}})}/></label>
      <button aria-expanded={preview} onClick={()=>setPreview(!preview)}>{preview?"Hide preview tools":"Preview changes"}</button>
      {preview&&<div className="preview-tools"><DraftPreview config={config}/><HardwarePreview config={config}/></div>}
    </div>
  </div>;
}

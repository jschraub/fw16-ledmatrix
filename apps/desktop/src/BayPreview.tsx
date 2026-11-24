import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Frame } from "./bindings/Frame";
import type { Config } from "./bindings/Config";
import type { PanelLayout } from "./bindings/PanelLayout";
import { PanelDisplay } from "./PanelMap";

export function BayPreview({config,panel}:{config:Config; panel:PanelLayout}) {
  const [frame,setFrame]=useState<Frame|null>(null);
  const [failed,setFailed]=useState(false);
  const [sample,setSample]=useState(false);
  useEffect(()=>{
    let current=true;
    let timer:ReturnType<typeof setTimeout>;
    const index=config.panels.findIndex(p=>p.name===panel.name);
    async function draw(){
      try {
        const frames=await invoke<Frame[]>("render_draft",{config,sample});
        if(current){setFrame(frames[index]??null);setFailed(!frames[index]);}
      } catch {if(current){setFrame(null);setFailed(true);}}
      if(current&&!sample) timer=setTimeout(draw,2000);
    }
    // Coalesce slider events and discard replies to previous drafts.
    timer=setTimeout(draw,80);
    return ()=>{current=false;clearTimeout(timer);};
  },[config,panel.name,sample]);
  const light=panel.brightness_override??config.brightness;
  const blank=frame&&!frame.pixels.some(Boolean);
  return <figure className="bay-output">
    {frame && <span style={{opacity:light.off?0:Math.sqrt(light.manual/255)}}><PanelDisplay frame={frame} label={`${panel.name} draft preview`}/></span>}
    <figcaption><strong>{sample?"Sample preview":"On-screen preview"}</strong>
      <small>{failed?"Preview unavailable":!frame?"Rendering…":light.off?"Panel set to sleep":blank?"No lit pixels in this layout":sample?"Example values":"Current readings"}</small></figcaption>
    {(sample||blank)&&!light.off&&<button onClick={()=>setSample(!sample)}>{sample?"Use current readings":"Try example values"}</button>}
  </figure>;
}

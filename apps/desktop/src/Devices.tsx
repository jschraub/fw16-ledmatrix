import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DeviceReply } from "./bindings/DeviceReply";
import type { PanelFrame } from "./bindings/PanelFrame";
import type { Request } from "./bindings/Request";
import { useActionNotice } from "./useActionNotice";
import { WorkspaceTitle } from "./WorkspaceTitle";

export function Devices({connected=true,panels=[],assign}:{connected?:boolean;panels?:PanelFrame[];assign?:()=>void}) {
  const [reply,setReply]=useState<DeviceReply|null>(null);
  const [scan,setScan]=useState(0);
  const [loading,setLoading]=useState(true);
  const [active,setActive]=useState<string|null>(null);
  const [notice,setNotice]=useActionNotice();
  const [failed,setFailed]=useState(false);
  useEffect(()=>{
    let current=true;
    setReply(null);setFailed(false);setLoading(connected);
    if(connected) void invoke<DeviceReply>("devices",{request:{type:"devices"}}).then(result=>{
      if(current){setFailed(result.message.startsWith("Serial enumeration failed"));setReply(result);}
    }).catch(()=>{if(current)setFailed(true);}).finally(()=>{if(current)setLoading(false);});
    return ()=>{current=false;};
  },[scan,connected]);
  async function identify(request:Extract<Request,{type:"identify"|"cancel_identify"}>) {
    if(request.type==="identify")setActive(request.port);
    try {
      const result=await invoke<DeviceReply>("devices",{request});
      if(request.type==="cancel_identify") setNotice({tone:"neutral",text:"Stopping identification."});
      else if(result.message.startsWith("Identification ended")) setNotice({tone:"success",text:"Identification complete."});
      else setNotice({tone:"error",text:result.message});
      setReply(result);
    } catch {setNotice({tone:"error",text:"Couldn’t identify the panel. Check its connection."});}
    finally {if(request.type==="identify")setActive(null);}
  }
  return <section aria-label="Physical devices">
    <WorkspaceTitle title="Devices" purpose="Hardware"/>
    <div className="section-line"><h3>Connected panels</h3>
      <button disabled={!connected||loading||!!active} onClick={()=>setScan(n=>n+1)}>{loading?"Scanning…":"Scan again"}</button></div>
    {!connected ? <p>Reconnect to discover panels.</p> : failed ? <p role="alert">Scan failed. Check device access and try again.</p> :
      !loading && !reply?.devices.length ? <p>No physical panels detected. Connect a panel and scan again.</p> : null}
    {connected && !failed && reply?.devices.map(device=>{
      const matches=device.location && reply.devices.filter(d=>d.location===device.location).length===1
        ? panels.filter(p=>p.binding===device.location) : [];
      const name=matches.length===1?`${matches[0].name} panel`:"Unassigned panel";
      return <article key={device.port} className="device-row"><div className="section-line">
        <div><h4 className="device-name">{name}</h4><small>{device.port}</small></div>
        {active===device.port ? <button onClick={()=>void identify({type:"cancel_identify",port:device.port})}>Stop identifying</button> :
          <button disabled={!!active} onClick={()=>void identify({type:"identify",port:device.port})}>Identify panel</button>}</div>
        <p className="device-state">{device.status==="open; identification path ready"?"Ready":device.status}</p>
        <details><summary>Device details</summary><dl className="facts">
          <dt>Firmware</dt><dd>{device.firmware??"Not queried"}</dd>
          <dt>Location</dt><dd>{device.location??"Unknown; identify before assigning"}</dd>
        </dl></details>
      </article>;
    })}
    <div className="action-notice" data-tone={notice?.tone} aria-live="polite">{notice?.text}</div>
    {assign && connected && <div className="workspace-bottom"><button onClick={assign}>Assign panels to sides</button></div>}
  </section>;
}

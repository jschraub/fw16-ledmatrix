import type { PanelLayout } from "./bindings/PanelLayout";
import type { Template } from "./bindings/Template";
import type { Widget } from "./bindings/Widget";
import type { Presentation } from "./bindings/Presentation";

const shapes: Record<Template,{title:string; parts:Presentation[]; labels:string[]}> = {
  full_gauge:{title:"One bar",parts:["gauge"],labels:["Bar"]},
  clock_gauge:{title:"Clock + bar",parts:["clock","gauge"],labels:["Time","Lower bar"]},
  number_two_gauges:{title:"Number + bars",parts:["number","gauge","gauge"],labels:["Top number","Left bar","Right bar"]},
};
const options = [
  ["battery","Battery"],["context","Session context"],["activity","Activity"],
  ["five_hour","Five-hour usage"],["weekly","Weekly usage"],
] as const;
function blank(presentation:Presentation):Widget {
  return {presentation,source:presentation==="clock"?"clock":"battery",provider:"claude",session_id:null,
    intensity:null,reversed:false,hide_leading_hour_zero:false,warning_threshold:null};
}
function Diagram({shape}:{shape:Template}) {
  return <svg aria-hidden="true" viewBox="0 0 20 46" width="23" height="51">
    <rect x=".5" y=".5" width="19" height="45" rx="3" fill="#101a14" stroke="#536957"/>
    {Array.from({length:77},(_,i)=>{
      const x=i%7,y=Math.floor(i/7);
      const mark=shape==="full_gauge"?x>=2&&x<=4&&y>=3:shape==="clock_gauge"?
        (y<4&&(x===1||x===4||((y===0||y===3)&&x===5)))||(y>=6&&x>=2&&x<=4):
        (y<3&&(x===1||x===4||y===0&&x===5))||(y>=5&&x===1)||(y>=7&&x===5);
      return mark?<circle key={i} cx={4+x*2} cy={5+y*3.5} r=".87" fill="currentColor"/>:null;
    })}
  </svg>;
}
export function LayoutComposer({panel,update}:{panel:PanelLayout;update:(p:Partial<PanelLayout>)=>void}) {
  function select(template:Template) {
    if(template===panel.template)return;
    const existing=panel.slots.filter((w):w is Widget=>w!==null);
    update({template,slots:shapes[template].parts.map(part=>{
      const match=existing.findIndex(w=>w.presentation===part);
      return match<0?blank(part):existing.splice(match,1)[0];
    })});
  }
  function replace(index:number,w:Widget|null){update({slots:panel.slots.map((old,i)=>i===index?w:old)});}
  return <section className="composer" aria-label="Panel layout">
    <h3>Layout</h3>
    <div className="shape-picker">{(Object.keys(shapes) as Template[]).map(shape=>
      <button key={shape} aria-pressed={panel.template===shape} onClick={()=>select(shape)}>
        <Diagram shape={shape}/><span>{shapes[shape].title}</span>
      </button>)}</div>
    <h3>Content</h3>
    {shapes[panel.template].parts.map((part,i)=>{
      const w=panel.slots[i],label=shapes[panel.template].labels[i];
      return <div className="content-row" key={`${panel.template}-${i}`}><label>{label}
        <select value={w?.source??""} onChange={e=>replace(i,e.target.value?
          {...(w??blank(part)),source:e.target.value as Widget["source"],session_id:null}:null)}>
          <option value="">Empty</option>{part==="clock"?<option value="clock">Clock</option>:
            options.map(([key,name])=><option key={key} value={key}>{name}</option>)}
        </select></label>
        {w&&w.source!=="battery"&&w.source!=="clock"&&<label>Provider for {label.toLowerCase()}
          <select value={w.provider} onChange={e=>replace(i,{...w,provider:e.target.value as Widget["provider"],session_id:null})}>
            <option value="opencode_openai">OpenAI</option><option value="claude">Claude</option>
          </select></label>}
      </div>;
    })}
  </section>;
}

// Three interaction studies of the same panel workspace, A/B/C via ?variant=.
// Illustrative output only. No native bridge, account access or saved settings.
import { useEffect, useRef, useState } from "react";
import "./studies.css";

const themes = ["Signal room", "Field notes", "Operator"] as const;
const sides = ["Left","Right"] as const;
type Side = typeof sides[number];
const widgets = ["Clock","Battery","Weekly usage"] as const;
type Widget = typeof widgets[number];
type Setting = { widget:Widget; light:number };
type Pair = Record<Side,Setting>;
const initial:Pair = {Left:{widget:"Battery",light:64},Right:{widget:"Clock",light:96}};
const values:Record<Widget,string> = {Clock:"21:54",Battery:"74%", "Weekly usage":"18%"};
const hints:Record<Widget,string> = {Clock:"Local time", Battery:"Charge remaining","Weekly usage":"Account allowance used"};
const letters:Record<string,string[]> = {
  "1":["010","110","010","010","111"],"2":["111","001","111","100","111"],
  "4":["101","101","111","001","001"],"5":["111","100","111","001","111"],
  "8":["111","101","111","101","111"],
};
function lit(widget:Widget,x:number,y:number) {
  const digit=(n:string,dx:number,dy:number)=>letters[n][y-dy]?.[x-dx]==="1";
  if(widget==="Clock") return digit("2",1,4)||digit("1",5,4)||digit("5",1,12)||digit("4",5,12);
  if(widget==="Battery") return x>=3 && x<=5 && y>=9;
  return digit("1",1,4)||digit("8",5,4)||(x>=3&&x<=5&&y>=29);
}
function Matrix({setting}:{setting:Setting}) {
  return <svg className="study-panel" viewBox="0 0 54 174" role="img" aria-label={`${setting.widget}, sample ${values[setting.widget]}`}>
    <rect x=".5" y=".5" width="53" height="173" rx="7" fill="var(--housing)" stroke="var(--rim)" />
    {Array.from({length:306},(_,i)=>{const x=i%9,y=Math.floor(i/9),on=lit(setting.widget,x,y);
      return <rect className="led" key={i} x={7.5+x*4.5} y={10.5+y*4.5} width="3" height="3" rx="1.5" fill={on?"var(--phosphor)":"var(--unlit)"} opacity={on ? .3+.7*setting.light/255 : 1} />;})}
  </svg>;
}
function Mark() {return <svg aria-hidden="true" viewBox="0 0 20 22" width="20" height="22">{[0,1,2,3,4].flatMap((y)=>[0,1,2,3].filter(x=>x===0||x===3||x===y||x===3-y).map(x=><circle key={`${x}${y}`} cx={3+x*4.5} cy={2+y*4.5} r="1.55" fill="currentColor"/>))}</svg>;}
function fromURL(){return Math.max(0,["A","B","C"].indexOf(new URLSearchParams(location.search).get("variant")??"A"));}
export function DesignStudies() {
  const [theme,setTheme]=useState(fromURL), [side,setSide]=useState<Side>("Right");
  const [pair,setPair]=useState<Pair>(initial),[draft,setDraft]=useState<Setting|null>(null);
  const [square,setSquare]=useState(false),[notice,setNotice]=useState("");
  const editorTitle=useRef<HTMLHeadingElement>(null);
  const active=draft??pair[side];
  function choose(n:number){const idx=(n+3)%3; setTheme(idx); const url=new URL(location.href);url.searchParams.set("variant","ABC"[idx]);history.replaceState(null,"",url);}
  useEffect(()=>{function keys(e:KeyboardEvent){if(e.altKey||e.metaKey||e.ctrlKey||(e.target as HTMLElement).closest("input,select,textarea,[contenteditable=true]"))return;
    if(e.key==="ArrowRight"||e.key==="ArrowLeft"){e.preventDefault();choose(theme+(e.key==="ArrowRight"?1:-1));}}
    window.addEventListener("keydown",keys);return()=>window.removeEventListener("keydown",keys);},[theme]);
  function open(s:Side,w?:Widget){setSide(s);setDraft({...pair[s],widget:w??pair[s].widget});setNotice("");requestAnimationFrame(()=>editorTitle.current?.focus());}
  function finish(save:boolean){if(save&&draft)setPair({...pair,[side]:draft});setDraft(null);setNotice(save?`${side} panel saved in this demo.`:"Draft discarded.");requestAnimationFrame(()=>document.getElementById("study-title")?.focus());}
  const controls=<><div className="control"><label htmlFor="widget">Content</label><select id="widget" value={active.widget} onChange={e=>setDraft({...active,widget:e.target.value as Widget})}>{widgets.map(w=><option key={w}>{w}</option>)}</select></div>
    <div className="control"><label htmlFor="level">Brightness <output>{Math.round(active.light/255*100)}%</output></label>
    <input id="level" type="range" min="1" max="255" value={active.light} onChange={e=>setDraft({...active,light:+e.target.value})}/></div></>;
  const sideSelect=<div className="side-select" aria-label="Target panel">{sides.map(s=><button key={s} aria-pressed={side===s} onClick={()=>{setSide(s);setNotice("");}}>{s} panel</button>)}</div>;
  return <div className={`study study-${"abc"[theme]} ${square?"square":""}`}>
    <div className="study-wrap"><header className="mast"><b><Mark/> Matrix Studio</b><span className="mono">FW16 &nbsp;/&nbsp; DESIGN STUDY 0{theme+1}</span></header>
    {draft ? <section className="editor-scene"><div className="edit-layout"><div className="focus-stage"><Matrix setting={active}/><small className="mono">ON-SCREEN PREVIEW</small></div>
      <div><p className="kicker">UNSAVED DRAFT</p><h1 ref={editorTitle} tabIndex={-1}>{side} panel<span>.</span></h1><p className="lede">Choose what appears here, then set the light.</p>
        {controls}<div className="ed-actions"><button className="primary" onClick={()=>finish(true)}>Save to demo</button><button onClick={()=>finish(false)}>Discard</button></div></div></div></section> :
    theme===0 ? <section><div className="hero"><p className="kicker"><i/> SIGNAL ROOM</p><h1 id="study-title" tabIndex={-1}>Your panels<span>.</span></h1><p className="lede">Two small displays. Choose one to shape its light.</p></div>
      <div className="pair-stage">{sides.map((s,i)=><button key={s} className="exhibit" onClick={()=>open(s)}><span className="coordinates mono">0{i+1} / {s.toUpperCase()} BAY</span><Matrix setting={pair[s]}/>
        <span className="exhibit-caption"><span><small>Configured content</small><strong>{pair[s].widget}</strong></span><em aria-hidden="true">↗</em></span></button>)}</div></section> :
    theme===1 ? <section className="catalog"><div className="catalog-heading"><p className="kicker">FIELD NOTES &nbsp; / &nbsp; WIDGET LIBRARY</p><h1 id="study-title" tabIndex={-1}>What should<br/>it show<span>?</span></h1>
      <p className="lede">Pick a destination, then a piece of information.</p>{sideSelect}</div>
      <div className="catalog-list">{widgets.map((w,i)=><button key={w} onClick={()=>open(side,w)}><span className="mono">FIG. 0{i+1}</span><span className="swatch"><Matrix setting={{widget:w,light:230}}/></span>
        <span><strong>{w}</strong><small>{hints[w]}</small><small>{sides.filter(s=>pair[s].widget===w).join(" + ") || "Not assigned"}</small></span><em>↗</em></button>)}</div></section> :
    <section className="console"><div className="console-head"><p className="kicker"><i/> OPERATOR &nbsp; / &nbsp; LIGHT DESK</p><h1 id="study-title" tabIndex={-1}>Panel mixer<span>_</span></h1><p className="lede">Select a channel to inspect its output.</p></div>
      <div className="console-body"><div className="channels">{sides.map((s,i)=><button key={s} aria-pressed={s===side} onClick={()=>{setSide(s);setNotice("");}}><small className="mono">CH : 0{i+1}</small><strong>{s} panel</strong><small>{pair[s].widget}</small></button>)}</div>
      <div className="monitor"><div className="monitor-data"><small className="mono">SAMPLE OUTPUT</small><strong>{values[pair[side].widget]}</strong><p>{hints[pair[side].widget]}</p><button className="primary" onClick={()=>open(side)}>Tune {side.toLowerCase()} panel →</button></div><Matrix setting={pair[side]}/></div></div></section>}
    <footer><span role="status">{notice || "Sample output · changes stay in this study."}</span><label><input type="checkbox" checked={square} onChange={e=>setSquare(e.target.checked)}/> Square LEDs</label></footer></div>
    <nav className="study-switch" aria-label="Compare design studies"><button onClick={()=>choose(theme-1)} aria-label="Previous design">←</button><span><small>COMPARE DESIGNS</small><b>{"ABC"[theme]} · {themes[theme]}</b></span><button onClick={()=>choose(theme+1)} aria-label="Next design">→</button></nav>
  </div>;
}

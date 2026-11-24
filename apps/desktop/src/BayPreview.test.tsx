import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { Config } from "./bindings/Config";
import { BayPreview } from "./BayPreview";

vi.mock("@tauri-apps/api/core",()=>({invoke:vi.fn()}));
afterEach(cleanup);
test("renders just the selected bay and ignores a previous draft's late reply",async()=>{
  const light={minimum:3,maximum:255,manual:32,off:false};
  const config:Config={schema_version:2,clock_intensity:180,brightness:light,panels:["right","left"].map(name=>({
    name,template:"full_gauge",binding:null,brightness_override:null,slots:[null]}))};
  const frame=(v:number)=>({width:9,height:34,pixels:Array<number>(306).fill(v)});
  let finish!:(frames:ReturnType<typeof frame>[])=>void;
  vi.mocked(invoke).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}))
    .mockResolvedValue([frame(0),frame(255)]);
  const {container,rerender}=render(<BayPreview config={config} panel={config.panels[1]}/>);
  await waitFor(()=>expect(invoke).toHaveBeenCalledWith("render_draft",{config,sample:false}));
  rerender(<BayPreview config={{...config,clock_intensity:220}} panel={config.panels[1]}/>);
  await screen.findByRole("img",{name:"left draft preview"});
  finish([frame(255),frame(0)]);await Promise.resolve();
  expect(screen.getAllByRole("img")).toHaveLength(1);
  expect((container.querySelector(".pixel") as HTMLElement).style.opacity).toBe("1");
});

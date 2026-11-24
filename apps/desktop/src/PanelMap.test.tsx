import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { PanelMap } from "./PanelMap";
import type { Snapshot } from "./bindings/Snapshot";
test("keeps physical side order and distinguishes assignment from live hardware detection", () => {
  const frame = {width:9,height:34,pixels:Array(306).fill(0)};
  const snapshot: Snapshot = {frame,preview:false,suspend_epoch:0,power:null,sequence:0,hour:0,minute:0,
    assistants:{usage:[],sessions:[],health:[]}, panels:[
      {name:"right",binding:"right-bay",base_brightness:32,brightness:61,frame,sources:["clock"]},
      {name:"left",binding:null,brightness:null,frame,sources:["battery"]},
    ]};
  const edit=vi.fn();
  render(<PanelMap snapshot={snapshot} connected edit={edit} setup={()=>{}} />);
  const [left,right]=screen.getAllByRole("button",{name:/Customize/});
  expect(left).toHaveTextContent("Unassigned"); expect(left).toHaveTextContent("Battery");
  expect(right).toHaveTextContent("Brightness 32"); expect(right).not.toHaveTextContent("61");
  expect(right).toHaveTextContent("Clock"); fireEvent.click(right);
  expect(edit).toHaveBeenCalledWith("right");
});

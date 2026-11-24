import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { LayoutComposer } from "./LayoutComposer";
import type { PanelLayout } from "./bindings/PanelLayout";

test("moving a bar into a split layout keeps its source, provider and styling", () => {
  const panel:PanelLayout={name:"left",binding:"left-bay",brightness_override:null,template:"full_gauge",
    slots:[{source:"weekly",provider:"opencode_openai",presentation:"gauge",intensity:99,reversed:true,session_id:null,
      warning_threshold:90,hide_leading_hour_zero:false}]};
  const update=vi.fn();
  render(<LayoutComposer panel={panel} update={update}/>);
  fireEvent.click(screen.getByRole("button",{name:"One bar"}));
  expect(update).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:"Clock + bar"}));
  expect(update).toHaveBeenCalledWith({template:"clock_gauge",slots:[expect.objectContaining({source:"clock"}),panel.slots[0]]});
});

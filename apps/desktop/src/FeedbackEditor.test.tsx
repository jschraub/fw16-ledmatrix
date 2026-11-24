import { useState } from "react";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import type { Config } from "./bindings/Config";
import { FeedbackEditor } from "./FeedbackEditor";
import { defaultBrightness } from "./BrightnessEditor";

afterEach(cleanup);
const panel = (side: string) => within(screen.getByRole("group", {name: `${side} panel`}));
test("custom pulse controls are per panel and preserve spread when changing the cycle", () => {
  render(<Harness />);
  for (const side of ["right", "left"]) fireEvent.click(panel(side).getByLabelText("Pulse while charging"));
  const mode = panel("right").getByLabelText("Pulse strength");
  expect(mode).toHaveValue("default");
  fireEvent.change(mode, {target:{value:"custom"}});
  fireEvent.change(panel("right").getByLabelText(/Brightness spread/), {target:{value:"96"}});
  fireEvent.change(panel("right").getByLabelText(/Full cycle/), {target:{value:"12"}});
  expect(panel("right").getByLabelText(/Brightness spread/)).toHaveValue(96);
  expect(panel("right").getByLabelText(/Full cycle/)).toHaveValue(12);
  expect(panel("left").getByLabelText("Pulse strength")).toHaveValue("default");
  fireEvent.change(mode, {target:{value:"extreme"}});
  expect(panel("right").queryByLabelText(/Full cycle/)).toBeNull();
});
function Harness() {
  const [draft, change] = useState<Config>({ schema_version: 2, clock_intensity: 180,
    brightness: defaultBrightness, panels: ["left", "right"].map(name => ({
      name, template: "full_gauge", slots: [null], binding: null, brightness_override: null,
    })) });
  return <FeedbackEditor config={draft} change={change} />;
}
test("each panel can opt into individual effects without enabling the other panel", () => {
  render(<Harness />);
  const left = panel("left").getByLabelText("Volume and mute duration");
  const right = panel("right").getByLabelText("Volume and mute duration");
  expect(left).toHaveValue("0");
  fireEvent.change(right, { target: { value: "6" } });
  const width = panel("right").getByLabelText("Volume and mute bar width");
  expect(width).toHaveValue("5");
  fireEvent.change(width, {target:{value:"7"}});
  fireEvent.change(panel("right").getByLabelText("Brightness change duration"), {target:{value:"3"}});
  expect(panel("right").getByLabelText("Brightness change bar width")).toHaveValue("5");
  expect(width).toHaveValue("7");
  fireEvent.click(panel("left").getByLabelText("Pulse while charging"));
  expect(right).toHaveValue("6");
  expect(left).toHaveValue("0");
  expect(panel("right").getByLabelText("Pulse while charging")).not.toBeChecked();
  expect(panel("left").getByLabelText("Brightness change duration")).toHaveValue("0");
});

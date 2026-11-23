import { useState } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import type { Config } from "./bindings/Config";
import { FeedbackEditor } from "./FeedbackEditor";
import { defaultBrightness } from "./BrightnessEditor";

afterEach(cleanup);
test("custom pulse controls are per panel and preserve spread when changing the cycle", () => {
  render(<Harness />);
  for (const side of ["right", "left"]) fireEvent.click(screen.getByLabelText(`Pulse whole ${side} panel while charging`));
  const mode = screen.getByLabelText("Pulse strength on right");
  expect(mode).toHaveValue("default");
  fireEvent.change(mode, {target:{value:"custom"}});
  fireEvent.change(screen.getByLabelText(/Brightness spread on right/), {target:{value:"96"}});
  fireEvent.change(screen.getByLabelText(/Full cycle on right/), {target:{value:"12"}});
  expect(screen.getByLabelText(/Brightness spread on right/)).toHaveValue(96);
  expect(screen.getByLabelText(/Full cycle on right/)).toHaveValue(12);
  expect(screen.getByLabelText("Pulse strength on left")).toHaveValue("default");
  fireEvent.change(mode, {target:{value:"extreme"}});
  expect(screen.queryByLabelText(/Full cycle on right/)).toBeNull();
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
  const left = screen.getByLabelText("Volume and mute on left");
  const right = screen.getByLabelText("Volume and mute on right");
  expect(left).toHaveValue("0");
  fireEvent.change(right, { target: { value: "6" } });
  const width = screen.getByLabelText("Volume and mute bar width on right");
  expect(width).toHaveValue("5");
  fireEvent.change(width, {target:{value:"7"}});
  fireEvent.change(screen.getByLabelText("Brightness change on right"), {target:{value:"3"}});
  expect(screen.getByLabelText("Brightness change bar width on right")).toHaveValue("5");
  expect(width).toHaveValue("7");
  fireEvent.click(screen.getByLabelText("Pulse whole left panel while charging"));
  expect(right).toHaveValue("6");
  expect(left).toHaveValue("0");
  expect(screen.getByLabelText("Pulse whole right panel while charging")).not.toBeChecked();
  expect(screen.getByLabelText("Brightness change on left")).toHaveValue("0");
});

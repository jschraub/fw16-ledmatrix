import { useState } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import type { Config } from "./bindings/Config";
import type { DisplaySources } from "./bindings/DisplaySources";
import { BrightnessEditor, defaultBrightness } from "./BrightnessEditor";
afterEach(cleanup);
function Harness({ sources }: {sources?: DisplaySources}) {
  const [config, change] = useState<Config>({ schema_version: 2, clock_intensity: 180, brightness: defaultBrightness, panels: [{ name: "left", binding: null, brightness_override: null, template: "full_gauge", slots: [null] }] });
  return <BrightnessEditor config={config} change={change} sources={sources} />;
}
test("display-off selection survives brightness source loss and can be used in manual mode", () => {
  const id = "eDP-2@fixture";
  const sources = {connected:[id], power_sources:[id], powered_off:[], backlights:{fixture:55}};
  const { rerender } = render(<Harness sources={sources}/>);
  const off = screen.getByLabelText("Sleep panels when this display turns off");
  const follow = screen.getByLabelText("Display and backlight");
  fireEvent.change(off, { target: { value:id } });
  fireEvent.change(follow, { target: { value:`${id}/fixture` } });
  fireEvent.change(screen.getByLabelText("Brightness response"), { target: { value:"quadratic" } });
  rerender(<Harness />);
  expect(screen.getByText(/manual fallback 32/)).toBeInTheDocument();
  expect(screen.getByLabelText("Brightness response")).toHaveValue("quadratic");
  fireEvent.change(follow, { target: { value:"" } });
  expect(off).toHaveValue(id);
});
test("overrides are explicit and independent from application defaults", () => {
  render(<Harness />);
  fireEvent.click(screen.getByLabelText("Override brightness for left"));
  fireEvent.change(screen.getByLabelText("left brightness manual"), { target: { value: "50" } });
  expect(screen.getByLabelText("Default brightness manual")).toHaveValue(32);
  expect(screen.getByLabelText("left brightness manual")).toHaveValue(50);
  fireEvent.click(screen.getByLabelText("Override brightness for left"));
  expect(screen.queryByLabelText("left brightness manual")).not.toBeInTheDocument();
});
test("off is an independent policy state rather than setting a zero level", () => {
  render(<Harness />); fireEvent.click(screen.getByLabelText("Default brightness off / sleep"));
  expect(screen.getByLabelText("Default brightness off / sleep")).toBeChecked();
  expect(screen.getByLabelText("Default brightness minimum")).toHaveValue(3);
  expect(screen.getByLabelText("Default brightness manual")).toHaveValue(32);
});

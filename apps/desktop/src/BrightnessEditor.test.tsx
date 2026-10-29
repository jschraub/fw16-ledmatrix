import { useState } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import type { Config } from "./bindings/Config";
import { BrightnessEditor, defaultBrightness } from "./BrightnessEditor";
afterEach(cleanup);
function Harness() {
  const [config, change] = useState<Config>({ schema_version: 2, clock_intensity: 180, brightness: defaultBrightness, panels: [{ name: "left", binding: null, brightness_override: null, template: "full_gauge", slots: [null] }] });
  return <BrightnessEditor config={config} change={change} />;
}
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
  expect(screen.getByText(/not a solid fill/)).toBeInTheDocument();
});

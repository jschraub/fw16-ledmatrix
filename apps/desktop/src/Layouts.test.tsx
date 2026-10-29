import { useState } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { Config } from "./bindings/Config";
import { DraftPreview, Layouts } from "./Layouts";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
const config: Config = { schema_version: 2, clock_intensity: 180, panels: [{ name: "left", template: "clock_gauge", slots: [
  { source: "clock", presentation: "clock", intensity: null, reversed: false, hide_leading_hour_zero: false, warning_threshold: null }, { source: "battery", presentation: "gauge", intensity: null, reversed: false, hide_leading_hour_zero: false, warning_threshold: null },
] }] };
function Harness() { const [value, setValue] = useState(config); return <Layouts config={value} change={setValue} />; }
test("guided slots permit empty/repeated widgets and template changes reset compatible slots", () => {
  render(<Harness />);
  fireEvent.change(screen.getByLabelText("Source for left slot 1"), { target: { value: "empty" } });
  expect(screen.queryByLabelText("Intensity for left slot 1")).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Template for left"), { target: { value: "full_gauge" } });
  expect(screen.getByLabelText("Source for left slot 1")).toHaveValue("battery");
  expect(screen.queryByLabelText("Source for left slot 2")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Add second panel" }));
  expect(screen.getByLabelText("Template for right")).toBeInTheDocument();
});
test("sample preview displays only daemon pixels and labels synthetic values", async () => {
  vi.mocked(invoke).mockResolvedValue([{ width: 9, height: 34, pixels: Array<number>(306).fill(0) }]);
  const { container } = render(<DraftPreview config={config} />);
  fireEvent.click(screen.getByRole("button", { name: "Preview with sample data" }));
  expect(await screen.findByRole("img", { name: "Draft preview for left" })).toBeInTheDocument();
  expect(screen.getByText(/SAMPLE DATA/)).toBeInTheDocument(); expect(container.querySelectorAll(".pixel")).toHaveLength(306);
  expect(invoke).toHaveBeenCalledWith("render_draft", { config, sample: true });
});

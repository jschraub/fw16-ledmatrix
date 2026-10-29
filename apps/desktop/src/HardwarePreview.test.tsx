import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { Config } from "./bindings/Config";
import { HardwarePreview } from "./HardwarePreview";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
const config: Config = { schema_version: 2, clock_intensity: 180, brightness: { minimum: 3, maximum: 255, manual: 32, off: false }, panels: [{ name: "left", binding: "bay-left", brightness_override: null, template: "full_gauge", slots: [null] }] };
test("cannot claim a hardware preview when no hardware is assigned", () => {
  render(<HardwarePreview config={{ ...config, panels: [{ ...config.panels[0], binding: null }] }} />);
  expect(screen.getByRole("button", { name: /Preview on assigned/ })).toBeDisabled(); expect(invoke).not.toHaveBeenCalled();
});
test("reports another client's lease without changing the draft", async () => {
  vi.mocked(invoke).mockResolvedValue({ status: "busy" });
  render(<HardwarePreview config={config} />); fireEvent.click(screen.getByRole("button", { name: /Preview on assigned/ }));
  expect(await screen.findByText(/Another client owns the preview/)).toBeInTheDocument();
  expect(invoke).toHaveBeenCalledWith("hardware_preview", { request: expect.objectContaining({ type: "begin_preview", config, seconds: 10 }) });
});

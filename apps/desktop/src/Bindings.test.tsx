import { useState } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { Config } from "./bindings/Config";
import { Bindings } from "./Bindings";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
function Harness() { const [config, change] = useState<Config>({ schema_version: 2, clock_intensity: 180, brightness: { minimum: 3, maximum: 255, manual: 32, off: false }, panels: [{ name: "left", binding: "saved-location", brightness_override: null, template: "full_gauge", slots: [null] }] }); return <Bindings config={config} change={change} />; }
test("retains disconnected assignments and refuses unknown identities", async () => {
  vi.mocked(invoke).mockResolvedValue({ devices: [{ port: "COM5", location: null, firmware: null, status: "detected" }], message: "fixture" });
  render(<Harness />); expect(screen.getByLabelText("Hardware for left")).toHaveValue("saved-location");
  fireEvent.click(screen.getByRole("button"));
  expect(await screen.findByRole("option", { name: /COM5/ })).toBeDisabled();
  expect(screen.getByLabelText("Hardware for left")).toHaveValue("saved-location");
});

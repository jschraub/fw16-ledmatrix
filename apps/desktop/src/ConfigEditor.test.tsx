import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ConfigEditor } from "./ConfigEditor";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
const saved = { revision: 0, config: { schema_version: 1, clock_intensity: 180 } };
test("loads a draft, applies through daemon validation and cancels local changes", async () => {
  vi.mocked(invoke).mockResolvedValue({ status: "read", saved, errors: [] });
  render(<ConfigEditor />); fireEvent.click(screen.getByRole("button", { name: "Load saved configuration" }));
  const input = await screen.findByLabelText("Clock intensity");
  fireEvent.change(input, { target: { value: "220" } });
  expect(invoke).toHaveBeenCalledTimes(1);
  vi.mocked(invoke).mockResolvedValue({ status: "applied", saved: { revision: 1, config: { ...saved.config, clock_intensity: 220 } }, errors: [] });
  fireEvent.click(screen.getByRole("button", { name: "Apply" }));
  expect(await screen.findByText("applied")).toBeInTheDocument();
  fireEvent.change(input, { target: { value: "200" } });
  fireEvent.click(screen.getByRole("button", { name: "Cancel draft" }));
  expect(input).toHaveValue(220);
});
test("retains the draft after a failed save", async () => {
  vi.mocked(invoke).mockResolvedValue({ status: "read", saved, errors: [] });
  render(<ConfigEditor />); fireEvent.click(screen.getByRole("button", { name: "Load saved configuration" }));
  const input = await screen.findByLabelText("Clock intensity"); fireEvent.change(input, { target: { value: "200" } });
  vi.mocked(invoke).mockResolvedValue({ status: "save_failed", saved, errors: [] });
  fireEvent.click(screen.getByRole("button", { name: "Apply" }));
  expect(await screen.findByText("save_failed")).toBeInTheDocument(); expect(input).toHaveValue(200);
});

test("shows conflicts and requires an explicit refresh before reapplying the retained draft", async () => {
  vi.mocked(invoke).mockResolvedValue({ status: "read", saved, errors: [] });
  render(<ConfigEditor />); fireEvent.click(screen.getByRole("button", { name: "Load saved configuration" }));
  const input = await screen.findByLabelText("Clock intensity"); fireEvent.change(input, { target: { value: "200" } });
  const current = { revision: 1, config: { ...saved.config, clock_intensity: 220 } };
  vi.mocked(invoke).mockResolvedValue({ status: "conflict", saved: current, errors: [] });
  fireEvent.click(screen.getByRole("button", { name: "Apply" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Another client saved revision 1");
  expect(input).toHaveValue(200);
  vi.mocked(invoke).mockResolvedValue({ status: "read", saved: current, errors: [] });
  fireEvent.click(screen.getByRole("button", { name: "Refresh saved state (keep draft)" }));
  expect(await screen.findByText(/Saved revision 1/)).toBeInTheDocument(); expect(input).toHaveValue(200);
  vi.mocked(invoke).mockResolvedValue({ status: "applied", saved: { revision: 2, config: { ...saved.config, clock_intensity: 200 } }, errors: [] });
  fireEvent.click(screen.getByRole("button", { name: "Apply" }));
  expect(await screen.findByText(/Saved revision 2/)).toBeInTheDocument();
  expect(invoke).toHaveBeenLastCalledWith("configuration", { request: { type: "apply_config", expected_revision: 1, config: { ...saved.config, clock_intensity: 200 } } });
});

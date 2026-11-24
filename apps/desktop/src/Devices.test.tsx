import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { Devices } from "./Devices";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
test("absent panels leave a useful hardware-free workflow", async () => {
  vi.mocked(invoke).mockResolvedValue({ devices: [], message: "Enumeration completed" });
  render(<Devices />);
  expect(await screen.findByText(/No physical panels detected/)).toBeInTheDocument();
});
test("a failed scan is not reported as an empty device list", async () => {
  vi.mocked(invoke).mockResolvedValue({devices:[],message:"Serial enumeration failed; check drivers and access."});
  render(<Devices/>);
  expect(await screen.findByRole("alert")).toHaveTextContent("Scan failed");
  expect(screen.queryByText(/No physical panels/)).toBeNull();
});
test("identification is explicit and unknown locations are not treated as bay identities", async () => {
  vi.mocked(invoke).mockResolvedValue({ devices: [{ port: "COM5", location: null, firmware: null, status: "detected" }], message: "Enumerated" });
  render(<Devices />);
  const identify = await screen.findByRole("button", { name: "Identify panel" });
  expect(screen.getByRole("heading",{name:"Unassigned panel"})).toBeVisible();
  fireEvent.click(identify);
  expect(invoke).toHaveBeenLastCalledWith("devices", { request: { type: "identify", port: "COM5" } });
});

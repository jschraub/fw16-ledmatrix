import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { Devices } from "./Devices";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
test("absent panels leave a useful hardware-free workflow", async () => {
  vi.mocked(invoke).mockResolvedValue({ devices: [], message: "Enumeration completed" });
  render(<Devices />); fireEvent.click(screen.getByRole("button", { name: "Discover physical panels" }));
  expect(await screen.findByText(/No physical panels detected/)).toBeInTheDocument();
});
test("identification is explicit and unknown locations are not treated as bay identities", async () => {
  vi.mocked(invoke).mockResolvedValue({ devices: [{ port: "COM5", location: null, firmware: null, status: "detected" }], message: "Enumerated" });
  render(<Devices />); fireEvent.click(screen.getByRole("button", { name: "Discover physical panels" }));
  const identify = await screen.findByRole("button", { name: "Identify panel" });
  expect(screen.getByText(/do not infer bay/)).toBeInTheDocument();
  fireEvent.click(identify);
  expect(invoke).toHaveBeenLastCalledWith("devices", { request: { type: "identify", port: "COM5" } });
});

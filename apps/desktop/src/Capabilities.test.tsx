import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { Capabilities } from "./Capabilities";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
test("reports no panels without claiming unsupported or unverified features work", async () => {
  vi.mocked(invoke).mockResolvedValue({ os: "linux", architecture: "x86_64", desktop: "unknown", panel_count: 0,
    capabilities: [{ name: "panels", state: "unavailable", detail: "No panels detected" }, { name: "identity", state: "not_verified", detail: "Needs real hardware" }] });
  render(<Capabilities />);
  fireEvent.click(screen.getByRole("button"));
  expect(await screen.findByText(/0 physical panels detected/)).toBeInTheDocument();
  expect(screen.getByText("not verified")).toBeInTheDocument();
});
test("offers retry after a failed probe without exposing raw errors", async () => {
  vi.mocked(invoke).mockRejectedValue("secret");
  render(<Capabilities />);
  fireEvent.click(screen.getByRole("button"));
  expect(await screen.findByRole("alert")).not.toHaveTextContent("secret");
  expect(screen.getByRole("button")).toBeEnabled();
});

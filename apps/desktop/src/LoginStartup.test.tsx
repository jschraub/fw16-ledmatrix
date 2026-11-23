import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { LoginStartup } from "./LoginStartup";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
test("checks without changing startup, then enables only on explicit selection", async () => {
  vi.mocked(invoke).mockResolvedValueOnce(false).mockResolvedValueOnce(true);
  render(<LoginStartup />);
  fireEvent.click(screen.getByText("Login startup"));
  expect(invoke).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("Check login startup"));
  fireEvent.click(await screen.findByText("Enable login startup"));
  expect(await screen.findByText("Login startup is enabled.")).toBeVisible();
  expect(vi.mocked(invoke).mock.calls).toEqual([["login_startup", {enabled:null}], ["login_startup", {enabled:true}]]);
});

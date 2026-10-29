import { useState } from "react";
import { cleanup, fireEvent, render, screen, waitFor, act } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { useDaemon } from "./useDaemon";
import { subscribe } from "./connection";
import type { Event } from "./bindings/Event";

vi.mock("./connection", async (original) => ({ ...await original<typeof import("./connection")>(), subscribe: vi.fn(), startDaemon: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

// A caller-owned draft proves reconnect does not reset future editor state;
// this does not add a pretend configuration editor before #13.
function EditorHarness() {
  const [draft, setDraft] = useState("");
  const daemon = useDaemon();
  return <><input aria-label="Draft" value={draft} onChange={(e) => setDraft(e.target.value)} />
    <p>{daemon.error?.code ?? (daemon.hello ? "connected" : "connecting")}</p>
    <button onClick={daemon.connect}>Retry</button></>;
}

test("retains caller drafts through disconnect, failed retry and daemon restart", async () => {
  let receive: (event: Event) => void = () => {};
  vi.mocked(subscribe).mockImplementation(async (callback) => { receive = callback; });
  render(<EditorHarness />);
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "unsaved settings" } });
  act(() => receive({ type: "error", code: "disconnected", message: "Connection closed" }));
  expect(await screen.findByText("disconnected")).toBeInTheDocument();
  vi.mocked(subscribe).mockRejectedValueOnce({ code: "unavailable", message: "Not running" });
  fireEvent.click(screen.getByRole("button"));
  expect(await screen.findByText("unavailable")).toBeInTheDocument();
  vi.mocked(subscribe).mockImplementation(async (callback) => {
    callback({ type: "hello", hello: { protocol_version: 2, daemon_version: "0.1.0", simulated: true, capabilities: ["clock_frames"] } });
  });
  fireEvent.click(screen.getByRole("button"));
  await waitFor(() => expect(screen.getByText("connected")).toBeInTheDocument());
  expect(screen.getByRole("textbox")).toHaveValue("unsaved settings");
});

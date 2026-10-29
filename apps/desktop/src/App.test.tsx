import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { Event } from "./bindings/Event";
import { subscribe, startDaemon } from "./connection";
import { App } from "./App";

vi.mock("./connection", async (original) => ({ ...await original<typeof import("./connection")>(), subscribe: vi.fn(), startDaemon: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const hello: Event = { type: "hello", hello: { protocol_version: 2, daemon_version: "0.1.0", simulated: true, capabilities: ["simulated_panel", "clock_frames"] } };
const pixels = Array<number>(306).fill(0);
pixels[10] = 180;
const frame: Event = { type: "frame", snapshot: { sequence: 42, hour: 12, minute: 34, preview: false, power: null, panels: [], frame: { width: 9, height: 34, pixels } } };

test("shows daemon pixels and handshake without inventing a clock in React", async () => {
  vi.mocked(subscribe).mockImplementation(async (receive) => { receive(hello); receive(frame); });
  const { container } = render(<App />);
  expect(await screen.findByText("Connected to daemon")).toBeInTheDocument();
  expect(screen.getByText("SIMULATED PANEL · NO HARDWARE")).toBeInTheDocument();
  expect(screen.getByRole("img", { name: "Simulated LED panel showing 12:34" })).toBeInTheDocument();
  expect(container.querySelectorAll(".pixel")).toHaveLength(306);
  expect((container.querySelectorAll(".pixel")[10] as HTMLElement).style.opacity).toBe(String(180 / 255));
  expect(screen.getByText(/Daemon 0.1.0/)).toBeInTheDocument();
});

test("reports a missing daemon and retries without starting or stopping it", async () => {
  vi.mocked(subscribe).mockRejectedValueOnce({ code: "unavailable", message: "No daemon is listening" });
  render(<App />);
  expect(await screen.findByRole("alert")).toHaveTextContent("No daemon");
  vi.mocked(subscribe).mockImplementation(async (receive) => { receive(hello); receive(frame); });
  fireEvent.click(screen.getByRole("button", { name: "Retry connection" }));
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Connected to daemon"));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("disconnected output is cleared instead of showing a frozen live clock", async () => {
  vi.mocked(subscribe).mockImplementation(async (receive) => {
    receive(hello); receive(frame); receive({ type: "error", code: "disconnected", message: "Daemon connection closed" });
  });
  render(<App />);
  expect(await screen.findByRole("alert")).toHaveTextContent("daemon connection closed");
  expect(screen.queryByRole("img")).not.toBeInTheDocument();
});

test("starts explicitly only when unavailable and connects when ready", async () => {
  vi.mocked(subscribe).mockRejectedValueOnce({ code: "unavailable", message: "No daemon is listening" });
  vi.mocked(startDaemon).mockResolvedValueOnce();
  render(<App />);
  const start = await screen.findByRole("button", { name: "Start daemon" });
  expect(startDaemon).not.toHaveBeenCalled();
  vi.mocked(subscribe).mockImplementation(async (receive) => { receive(hello); receive(frame); });
  fireEvent.click(start);
  expect(await screen.findByRole("img")).toBeInTheDocument();
  expect(startDaemon).toHaveBeenCalledTimes(1);
});

test("never offers to replace an incompatible daemon", async () => {
  vi.mocked(subscribe).mockRejectedValueOnce({ code: "incompatible", message: "Incompatible daemon; update matching binaries" });
  render(<App />);
  expect(await screen.findByRole("alert")).toHaveTextContent("incompatible protocol");
  expect(screen.queryByRole("button", { name: "Start daemon" })).not.toBeInTheDocument();
  expect(startDaemon).not.toHaveBeenCalled();
});

test("does not echo arbitrary native exceptions into diagnostics", async () => {
  vi.mocked(subscribe).mockRejectedValueOnce("private/path token=secret");
  render(<App />);
  expect(await screen.findByRole("alert")).toHaveTextContent("native desktop bridge is unavailable");
  expect(screen.queryByText(/token=secret/)).not.toBeInTheDocument();
});

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { Event } from "./bindings/Event";
import { subscribe } from "./connection";
import { App } from "./App";

vi.mock("./connection", () => ({ subscribe: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const hello: Event = { type: "hello", hello: { protocol_version: 1, daemon_version: "0.1.0", simulated: true, capabilities: ["simulated_panel", "clock_frames"] } };
const pixels = Array<number>(306).fill(0);
pixels[10] = 180;
const frame: Event = { type: "frame", snapshot: { sequence: 42, hour: 12, minute: 34, frame: { width: 9, height: 34, pixels } } };

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
  vi.mocked(subscribe).mockRejectedValueOnce("No daemon at local endpoint");
  render(<App />);
  expect(await screen.findByRole("alert")).toHaveTextContent("No daemon");
  vi.mocked(subscribe).mockImplementation(async (receive) => { receive(hello); receive(frame); });
  fireEvent.click(screen.getByRole("button", { name: "Retry connection" }));
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Connected to daemon"));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("disconnected output is cleared instead of showing a frozen live clock", async () => {
  vi.mocked(subscribe).mockImplementation(async (receive) => {
    receive(hello); receive(frame); receive({ type: "error", message: "Daemon connection closed" });
  });
  render(<App />);
  expect(await screen.findByRole("alert")).toHaveTextContent("Daemon connection closed");
  expect(screen.queryByRole("img")).not.toBeInTheDocument();
});

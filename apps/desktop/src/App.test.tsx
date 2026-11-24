import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { Event } from "./bindings/Event";
import { subscribe, startDaemon } from "./connection";
import { App } from "./App";
import { invoke } from "@tauri-apps/api/core";
import userEvent from "@testing-library/user-event";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

vi.mock("./connection", async (original) => ({ ...await original<typeof import("./connection")>(), subscribe: vi.fn(), startDaemon: vi.fn() }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

test("keyboard reaches device, display, layout, integration and support workflows and retains a draft on reconnect", async () => {
  const user = userEvent.setup();
  let publish!: (e: Event) => void;
  vi.mocked(subscribe).mockImplementation(async receive => { publish = receive; receive(hello); receive(frame); });
  vi.mocked(invoke).mockImplementation(async command => command === "devices" ? {message:"Detected",devices:[]} :
    {status:"read",errors:[],saved:{revision:1,config:{schema_version:2,clock_intensity:180,panels:[],brightness:{manual:32,minimum:3,maximum:255,off:false}}}});
  render(<App />);
  async function focus(element: HTMLElement) {
    for (let i=0; document.activeElement !== element && i<80; i++) await user.tab();
    expect(element).toHaveFocus();
  }
  async function activate(name: string) {
    if (["Devices","Configuration","Integrations","Diagnostics"].includes(name)) {
      await focus(screen.getByRole("button",{name:"Workspace"})); await user.keyboard("{Enter}");
    }
    await focus(screen.getByRole("button",{name})); await user.keyboard("{Enter}");
  }
  await activate("Devices"); await activate("Scan again");
  await activate("Configuration");
  const input = await screen.findByLabelText("Clock intensity"); await focus(input);
  await user.keyboard("{ArrowUp}");
  await user.clear(input); await user.keyboard("160");
  await activate("Brightness"); await focus(screen.getByLabelText("Display and backlight"));
  await activate("Integrations"); expect(screen.getByRole("heading",{name:"OpenAI"})).toBeVisible();
  await activate("Diagnostics"); await activate("Generate redacted report");
  const report = await screen.findByLabelText<HTMLTextAreaElement>("Redacted report (JSON)");
  expect(JSON.parse(report.value).connection).toBe("connected");
  act(() => publish({type:"error",code:"disconnected",message:"closed"}));
  await activate("Retry connection"); await activate("Configuration"); await activate("Layout");
  expect(screen.getByLabelText("Clock intensity")).toHaveValue(160);
});

const hello: Event = { type: "hello", hello: { protocol_version: 2, daemon_version: "0.1.0", simulated: true, capabilities: ["simulated_panel", "clock_frames"] } };
const pixels = Array<number>(306).fill(0);
pixels[10] = 180;
const frame: Event = { type: "frame", snapshot: { suspend_epoch: 0, sequence: 42, hour: 12, minute: 34, preview: false, assistants: { usage: [], sessions: [], health: [] }, power: null, panels: [], frame: { width: 9, height: 34, pixels } } };

test("shows daemon pixels and handshake without inventing a clock in React", async () => {
  vi.mocked(subscribe).mockImplementation(async (receive) => { receive(hello); receive(frame); });
  const { container } = render(<App />);
  expect(await screen.findByText("Connected to daemon")).toBeInTheDocument();
  expect(screen.getByText("Simulated output")).toBeInTheDocument();
  expect(screen.getByRole("img", { name: "Simulated LED panel showing 12:34" })).toBeInTheDocument();
  expect(container.querySelectorAll(".pixel")).toHaveLength(306);
  expect((container.querySelectorAll(".pixel")[10] as HTMLElement).style.opacity).toBe(String(180 / 255));
  expect(screen.getByText(/Daemon 0.1.0/)).toBeInTheDocument();
});

test("navigation reveals one task at a time and keeps unsaved configuration", async () => {
  vi.mocked(subscribe).mockImplementation(async receive => { receive(hello); receive(frame); });
  vi.mocked(invoke).mockResolvedValue({ status: "read", errors: [], saved: {
    revision: 4, config: { schema_version: 2, clock_intensity: 180, panels: [] },
  } });
  render(<App />);
  const go = (name: string) => {
    if (["Configuration","Diagnostics"].includes(name)) fireEvent.click(screen.getByRole("button",{name:"Workspace"}));
    fireEvent.click(screen.getByRole("button", { name }));
  };
  expect(screen.queryByRole("button", { name: "Load saved configuration" })).toBeNull();
  go("Configuration");
  fireEvent.change(await screen.findByLabelText("Clock intensity"), { target: { value: "160" } });
  go("Brightness");
  expect(screen.getByLabelText("Clock intensity")).not.toBeVisible();
  go("Diagnostics");
  expect(screen.getByRole("button", { name: "Probe this machine" })).toBeVisible();
  go("Configuration"); go("Layout");
  expect(screen.getByLabelText("Clock intensity")).toHaveValue(160);
  expect(invoke).toHaveBeenCalledTimes(1);
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

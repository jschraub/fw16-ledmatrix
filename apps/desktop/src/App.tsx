import { useDaemon } from "./useDaemon";
import { Capabilities } from "./Capabilities";
import { ConfigEditor } from "./ConfigEditor";
import { Devices } from "./Devices";
import { Integrations } from "./Integrations";
import { useState, useRef } from "react";
import { ViewNav } from "./ViewNav";

const views = ["Overview", "Configuration", "Devices", "Integrations", "Diagnostics"] as const;

export function App() {
  const { hello, snapshot, error, connecting, starting, connect, start } = useDaemon();
  const [view, setView] = useState<typeof views[number]>("Overview");
  const [selected, setSelected] = useState<string | null>(null);
  const editor = useRef<{ loadIfNeeded(): void }>(null);

  const time = snapshot
    ? `${String(snapshot.hour).padStart(2, "0")}:${String(snapshot.minute).padStart(2, "0")}`
    : "—";

  return (
    <main>
      <header>
        <p className="eyebrow">FRAMEWORK LAPTOP 16</p>
        <h1>LED Matrix</h1>
      </header>
      <section className="status" aria-label="Daemon connection">
        <span className={`dot ${hello ? "online" : ""}`} aria-hidden="true" />
        <span role="status">{connecting ? "Connecting…" : hello ? "Connected to daemon" : "Daemon disconnected"}</span>
        {!connecting && !hello && <button onClick={connect} disabled={starting}>Retry connection</button>}
        {!hello && !connecting && (error?.code === "unavailable" || error?.code === "start_failed") && (
          <button onClick={() => void start()} disabled={starting}>{starting ? "Starting…" : "Start daemon"}</button>
        )}
      </section>
      {error && <div role="alert" className="error">{error.message}
        <details><summary>Connection diagnostics</summary>
          <p>Code: <code>{error.code}</code>. Transport: user-local socket/pipe. No network listener.</p>
          <p>Build <code>cargo build -p matrix-daemon</code> before using Start daemon in development, or run <code>cargo run -p matrix-daemon</code> separately.</p>
          <p>Diagnostics exclude endpoint paths, usernames, credentials, and message payloads. No running process will be stopped or replaced.</p>
        </details>
      </div>}
      <ViewNav label="Main navigation" items={views} current={view} select={item => { setView(item); setSelected(null); }} />
      <div hidden={view !== "Overview"}>
      <section className="preview" aria-labelledby="preview-heading">
        {snapshot?.preview && <p role="status">TEMPORARY HARDWARE PREVIEW · not saved</p>}
        <div>
          <span className="badge">{snapshot?.panels.some((p) => p.binding !== null) ? "PANEL PREVIEW · CHECK DEVICE STATUS" : "SIMULATED PANEL · NO HARDWARE"}</span>
          <h2 id="preview-heading">Your panels</h2>
          {snapshot?.panels.length ? <p>Select a panel to edit its widgets or brightness.</p> : null}
          <p className="time">{time}</p>
          {snapshot?.power && <p className="muted">Battery source: {snapshot.power.status}
            {snapshot.power.percent !== null && <> · {snapshot.power.percent}%</>}
            {snapshot.power.charging !== null && <> · {snapshot.power.charging ? "charging" : "not charging"}</>}
            {snapshot.power.ac !== null && <> · AC {snapshot.power.ac ? "connected" : "disconnected"}</>}
            </p>}
        </div>
        {snapshot?.panels.length ? <div className="overview-panels">
          {[...snapshot.panels].sort((a, b) => a.name.localeCompare(b.name)).map(panel =>
            <button key={panel.name} className="bay-tile" aria-label={`Customize ${panel.name} panel`} aria-pressed={selected === panel.name}
              onClick={() => { setSelected(panel.name); editor.current?.loadIfNeeded(); }}>
              <strong>{panel.name} panel</strong><small>{panel.binding ? "Assigned" : "Simulated"}</small>
              <span className="panel" role="img" aria-label={`Live logical panel ${panel.name}`}>
                <span className="pixels" aria-hidden="true">{panel.frame.pixels.map((value, index) =>
                  <span key={index} className="pixel" style={{ opacity: value ? value / 255 : 0.06 }} />)}</span>
              </span>
              <small>Brightness: {panel.brightness ?? "off / sleep"}</small>
            </button>)}
        </div> : snapshot ? (
          <div className="panel" role="img" aria-label={`Simulated LED panel showing ${time}`}>
            <div className="pixels" aria-hidden="true">
              {snapshot.frame.pixels.map((value, index) => (
                <span key={index} className="pixel" style={{ opacity: value ? value / 255 : 0.06 }} />
              ))}
            </div>
          </div>
        ) : <div className="placeholder">{hello ? "Waiting for first frame…" : "Connect to view the panel"}</div>}
      </section>
      <footer>Closing this window leaves the daemon running.</footer>
      </div>
      <div hidden={view !== "Configuration" && !(view === "Overview" && selected)}><ConfigEditor ref={editor} panelName={selected} /></div>
      <div hidden={view !== "Devices"}><Devices /></div>
      <div hidden={view !== "Integrations"}>{snapshot ? <Integrations state={snapshot.assistants} /> : <p>Connect to the daemon to view assistant integrations.</p>}</div>
      <div hidden={view !== "Diagnostics"}>
        <Capabilities />
        {hello && <details><summary>Connection details</summary>
          <p>Daemon {hello.daemon_version} · protocol {hello.protocol_version}<br />Capabilities: {hello.capabilities.join(", ")}</p>
          {snapshot && <p>Update {snapshot.sequence} · Screen brightness is not calibrated to physical LEDs.</p>}
        </details>}
      </div>
    </main>
  );
}

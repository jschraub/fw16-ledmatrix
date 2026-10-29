import { useDaemon } from "./useDaemon";
import { Capabilities } from "./Capabilities";
import { ConfigEditor } from "./ConfigEditor";
import { Devices } from "./Devices";

export function App() {
  const { hello, snapshot, error, connecting, starting, connect, start } = useDaemon();

  const time = snapshot
    ? `${String(snapshot.hour).padStart(2, "0")}:${String(snapshot.minute).padStart(2, "0")}`
    : "—";

  return (
    <main>
      <header>
        <p className="eyebrow">FRAMEWORK LAPTOP 16</p>
        <h1>LED Matrix</h1>
        <p className="subtitle">Independent daemon · desktop preview</p>
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
      <section className="preview" aria-labelledby="preview-heading">
        <div>
          <span className="badge">SIMULATED PANEL · NO HARDWARE</span>
          <h2 id="preview-heading">Clock</h2>
          <p className="time">{time}</p>
          <p>Live local time. Pixels rendered by Rust, not React.</p>
          <p className="muted">9 × 34 · 8-bit greyscale<br />Screen brightness is not calibrated to physical LEDs.</p>
          {hello && <p className="muted">Daemon {hello.daemon_version} · protocol {hello.protocol_version}<br />Capabilities: {hello.capabilities.join(", ")}</p>}
          {snapshot && <p className="muted">Update {snapshot.sequence}</p>}
          {snapshot?.power && <p className="muted">Battery source: {snapshot.power.status}
            {snapshot.power.percent !== null && <> · {snapshot.power.percent}%</>}
            {snapshot.power.charging !== null && <> · {snapshot.power.charging ? "charging" : "not charging"}</>}
            {snapshot.power.ac !== null && <> · AC {snapshot.power.ac ? "connected" : "disconnected"}</>}
            <br />Observed: {snapshot.power.updated_at}. Unavailable/error data stays blank.</p>}
        </div>
        {snapshot ? (
          <div className="panel" role="img" aria-label={`Simulated LED panel showing ${time}`}>
            <div className="pixels" aria-hidden="true">
              {snapshot.frame.pixels.map((value, index) => (
                <span key={index} className="pixel" style={{ opacity: value ? value / 255 : 0.06 }} />
              ))}
            </div>
          </div>
        ) : <div className="placeholder">{hello ? "Waiting for first frame…" : "Connect to view the panel"}</div>}
      </section>
      <footer>Closing this window leaves the daemon running. Reopen to see its latest frame.</footer>
      <Capabilities />
      <Devices />
      <ConfigEditor />
    </main>
  );
}

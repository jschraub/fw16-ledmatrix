import { useCallback, useEffect, useRef, useState } from "react";
import type { Hello } from "./bindings/Hello";
import type { Snapshot } from "./bindings/Snapshot";
import { subscribe } from "./connection";

export function App() {
  const [hello, setHello] = useState<Hello | null>(null);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(true);
  const generation = useRef(0);

  const connect = useCallback(() => {
    const current = ++generation.current;
    setConnecting(true);
    setError(null);
    setHello(null);
    setSnapshot(null);
    void subscribe((event) => {
      if (current !== generation.current) return;
      if (event.type === "hello") {
        setHello(event.hello);
        setConnecting(false);
      } else if (event.type === "frame") {
        setSnapshot(event.snapshot);
      } else {
        setError(event.message);
        setConnecting(false);
        setHello(null);
        setSnapshot(null);
      }
    }).catch((reason: unknown) => {
      if (current !== generation.current) return;
      setError(String(reason));
      setConnecting(false);
    });
  }, []);

  useEffect(() => {
    connect();
    return () => { generation.current++; };
  }, [connect]);

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
        {!connecting && !hello && <button onClick={connect}>Retry connection</button>}
      </section>
      {error && <div role="alert" className="error">{error}<p>Start <code>cargo run -p matrix-daemon</code> in a separate terminal, then retry.</p></div>}
      <section className="preview" aria-labelledby="preview-heading">
        <div>
          <span className="badge">SIMULATED PANEL · NO HARDWARE</span>
          <h2 id="preview-heading">Clock</h2>
          <p className="time">{time}</p>
          <p>Live local time. Pixels rendered by Rust, not React.</p>
          <p className="muted">9 × 34 · 8-bit greyscale<br />Screen brightness is not calibrated to physical LEDs.</p>
          {hello && <p className="muted">Daemon {hello.daemon_version} · protocol {hello.protocol_version}<br />Capabilities: {hello.capabilities.join(", ")}</p>}
          {snapshot && <p className="muted">Update {snapshot.sequence}</p>}
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
    </main>
  );
}

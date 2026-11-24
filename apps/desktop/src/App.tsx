import { useDaemon } from "./useDaemon";
import { Capabilities } from "./Capabilities";
import { ConfigEditor, type EditorActions } from "./ConfigEditor";
import { Devices } from "./Devices";
import { Integrations } from "./Integrations";
import { useState, useRef } from "react";
import { ViewNav } from "./ViewNav";
import { LoginStartup } from "./LoginStartup";
import { DiagnosticExport } from "./DiagnosticExport";
import { PanelMap } from "./PanelMap";

const views = ["Overview", "Configuration", "Devices", "Integrations", "Diagnostics"] as const;

export function App() {
  const { hello, snapshot, error, connecting, starting, connect, start } = useDaemon();
  const [view, setView] = useState<typeof views[number]>("Overview");
  const [selected, setSelected] = useState<string | null>(null);
  const editor = useRef<EditorActions>(null);

  const back = useRef<HTMLButtonElement>(null);
  const [menu, setMenu] = useState(false);
  const workspace = useRef<HTMLButtonElement>(null);
  function edit(name: string) {
    setSelected(name); editor.current?.loadIfNeeded();
    requestAnimationFrame(()=>back.current?.focus());
  }

  return (
    <main>
      <header className="mast">
        <h1><span className="matrix-mark" aria-hidden="true"/>Matrix Studio</h1>
        <button ref={workspace} aria-expanded={menu} aria-controls="workspace-nav" onClick={()=>setMenu(!menu)}>Workspace</button>
      </header>
      <div id="workspace-nav" hidden={!menu}><ViewNav label="Main navigation" items={views} current={view} select={item => { setView(item); setSelected(null); setMenu(false); workspace.current?.focus(); }} /></div>
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
      <div hidden={view !== "Overview" || !!selected}>
      <PanelMap snapshot={snapshot} connected={!!hello} edit={edit} setup={()=>setView("Devices")} />
      <footer>Closing this window leaves the daemon running.</footer>
      </div>
      <div hidden={view !== "Configuration" && !(view === "Overview" && selected)}>
        {selected && <button ref={back} onClick={()=>{setSelected(null); requestAnimationFrame(()=>document.getElementById("map-title")?.focus());}}>← Your panels</button>}
        <ConfigEditor ref={editor} panelName={selected} sources={snapshot?.display_sources} />
        {view === "Configuration" && <LoginStartup />}
      </div>
      <div hidden={view !== "Devices"}><Devices />
        <button onClick={()=>{setView("Configuration"); setSelected(null); editor.current?.openAssignments();}}>Assign panels to sides</button>
      </div>
      <div hidden={view !== "Integrations"}>{snapshot ? <Integrations state={snapshot.assistants} /> : <p>Connect to the daemon to view assistant integrations.</p>}</div>
      <div hidden={view !== "Diagnostics"}>
        <DiagnosticExport snapshot={snapshot} failure={error?.code} />
        <Capabilities />
        {hello && <details><summary>Connection details</summary>
          <p>Daemon {hello.daemon_version} · protocol {hello.protocol_version}<br />Capabilities: {hello.capabilities.join(", ")}</p>
          {snapshot && <p>Update {snapshot.sequence} · Screen brightness is not calibrated to physical LEDs.</p>}
        </details>}
      </div>
    </main>
  );
}

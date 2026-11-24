export function WorkspaceTitle({title, purpose}: {title: string; purpose: string}) {
  return <header className="workspace-title">
    <p className="kicker">{purpose}</p>
    <h2>{title}<span aria-hidden="true">.</span></h2>
  </header>;
}

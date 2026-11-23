import type { Config } from "./bindings/Config";
import type { DisplaySources } from "./bindings/DisplaySources";
import type { DisplaySelection } from "./bindings/DisplaySelection";

export function DisplayFollow({ config, sources, change }: {
  config: Config; sources?: DisplaySources; change: (config: Config) => void;
}) {
  const selected = config.follow_display;
  const choices = (sources?.connected ?? []).flatMap(connector =>
    Object.keys(sources?.backlights ?? {}).map(backlight => ({ connector, backlight })));
  const key = (s: DisplaySelection) => `${s.connector}/${s.backlight}`;
  if (selected && !choices.some(s => key(s) === key(selected))) choices.push(selected);
  const level = selected && sources?.connected.includes(selected.connector)
    ? sources.backlights[selected.backlight] : null;
  return <fieldset><legend>Display-follow brightness</legend>
    <label>Display and backlight <select value={selected ? key(selected) : ""}
      onChange={e => change({ ...config, follow_display: choices.find(s => key(s) === e.target.value) })}>
      <option value="">Manual brightness</option>
      {choices.map(s => <option key={key(s)} value={key(s)}>{s.connector} · {s.backlight}</option>)}
    </select></label>
    {selected ? <p>{level == null ? `Source unavailable · manual fallback ${config.brightness.manual}` : `Source level: ${level}%`}
      . Inherited brightness follows the display between your minimum and maximum. Panel overrides stay manual.</p>
      : <p>{choices.length ? "Choose the backlight that controls your display. The pairing is not detected automatically." : "No display and backlight pair reported."}</p>}
  </fieldset>;
}

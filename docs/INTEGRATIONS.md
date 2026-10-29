# Assistant metadata feeds

The daemon consumes versioned **metadata only** from per-user atomic JSON
snapshots. Producers for actual clients are separate follow-on slices; a missing
producer leaves its slots blank. No credentials or conversation text belong in
these files. Unknown JSON fields are rejected, not displayed or logged.

The root is `MATRIX_FEEDS`, otherwise `fw16-matrix/feeds` below the user runtime
directory, falling back to platform local data (`XDG_DATA_HOME` / local app data).
Use `claude/<instance-id>.json` and `opencode/<instance-id>.json`. IDs contain only
ASCII letters/digits/hyphens/underscores, at most 128 bytes. File stem must match
instance ID. Unix snapshots must be owner-private (0600); symlinks are rejected.
Do not edit external client settings automatically.

Schema 1 contains: `schema_version`, `instance_id`, `provider` (`claude` or
`opencode_openai`), `heartbeat_ms` (Unix milliseconds), and `sessions`. Each session
contains `session_id`, nullable `parent_id`, nullable integer `context_percent`,
`busy`, and `last_activity_ms`. Context may exceed 99; the display honestly uses
`XX` instead of a false clamped number. Heartbeats must be fresh within 60 seconds;
future timestamps over five seconds ahead are rejected. Publishing a heartbeat
must not change activity ordering. Only main sessions are eligible.

Limits: 64 KiB per file, 16 sessions per file, 16 instance files per provider,
32 distinct published main sessions per provider. Limit/error status is visible;
malformed files do not suppress healthy peers or another provider. Repeated
widgets share the single normalized scan. Context/activity select the latest
active main session for that widget's provider; a missing/expired pin stays blank
instead of silently switching to another conversation. Busy context numbers use
emphasis; standalone activity widgets remain independent of context availability.

The Integrations screen shows source health, eligible IDs, context/activity and
separate heartbeat/activity timestamps. Selection follows activity, not terminal
focus or a persistent server's attached UI. Source failures never stop the clock.
Quota retrieval is a separate adapter and must not be inferred from session feeds.

## OpenCode V2 producer

`integrations/opencode` targets `@opencode/plugin` 2.2.1. The POC's V1 plugin is
preserved, not installed or loaded by this application. Explicit setup:

1. Run `npm ci --ignore-scripts` in `integrations/opencode`.
2. In your chosen OpenCode `opencode.json(c)`, preserve existing entries and add
   the **absolute directory path** to this package under `plugins`:

   ```jsonc
   {
     "$schema": "https://opencode.ai/config.json",
     "plugins": ["/absolute/path/to/fw16-ledmatrix/integrations/opencode"]
   }
   ```

   On Windows use a JSON-escaped path or forward slashes, e.g.
   `C:/src/fw16-ledmatrix/integrations/opencode`. Paths must name the directory,
   not `index.js`. Do not install both the POC and this producer.
3. Config/plugin changes hot reload in V2; use `/reload` for an explicit reload.
   Confirm plugin ID `fw16-matrix.metadata` in the active plugin list, then inspect
   the matrix Integrations screen. No user settings are edited automatically.
4. Select an OpenAI main session, submit a request, and observe context/activity.
   Remove the plugin entry and reload to uninstall. Cleanup deletes only this
   instance's snapshot. Abrupt server death leaves a snapshot expiring in 60 s.

Every five seconds a single coalesced job reads public V2 session/model APIs.
It considers the newest 16 unarchived main sessions at the plugin location plus
up to 16 active drains, and publishes the 16 most recently active eligible
sessions. Older inactive sessions outside that window cannot be pinned until
observed again. Other locations, child sessions and non-OpenAI selections are
excluded. Latest assistant response input/output/reasoning/cache-read/cache-write
usage is divided by that response's current model context limit (including model
overrides); cumulative session tokens are never used. Unknown usage/model limits
stay unavailable. Message text can transit SDK memory but is never copied into
the metadata file or logged. Each SDK sweep has a five-second cancellation
signal; overlapping polls are skipped, not queued. Failed sweeps do not renew
the previous heartbeat. A failed individual session is omitted until recovery.

Activity timestamps come from actual latest user/assistant message times, never
the heartbeat or generic session rename/view updates. Busy means a foreground
drain owned by that server, **not terminal focus**. A persistent server continues
publishing while no TUI is attached. Several servers/locations own separate UUID
snapshots. Set `MATRIX_FEEDS` to the same absolute directory in the **server's**
environment and the daemon if their default user paths differ.

Unix files/directories are private (0600/0700), and unsafe existing directories
are rejected without chmod or replacement. Windows uses inherited per-user local
data directory ACLs; custom shared paths need operator ACL review. JS/Rust
contract and fake-client tests do not establish interactive OpenCode activation,
Windows ACLs, or physical panel readability; those require separate qualification.

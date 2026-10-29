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

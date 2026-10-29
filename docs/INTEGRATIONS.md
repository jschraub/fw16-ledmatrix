# Assistant metadata feeds

The daemon consumes versioned **metadata only** from per-user atomic JSON
snapshots. Optional Claude and OpenCode producers are installed explicitly below;
a missing producer leaves its slots blank. No credentials or conversation text belong in
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

## Claude Code Rust helper

Build/install explicitly with `cargo install --path crates/matrix-claude --locked`.
Ensure `matrix-claude` (`matrix-claude.exe` on Windows) is on Claude's PATH.
The helper does not edit settings or replace your status-line renderer.
Back up your settings and merge these entries, preserving unrelated fields,
existing hooks and `statusLine` options such as padding:

```json
{
  "statusLine": {
    "type": "command",
    "command": "matrix-claude statusline -- bash /home/you/.claude/statusline.sh",
    "refreshInterval": 15
  },
  "hooks": {
    "SessionStart": [{ "hooks": [{ "type": "command", "command": "matrix-claude", "args": ["hook"], "timeout": 2 }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "matrix-claude", "args": ["hook"], "timeout": 2 }] }],
    "Stop": [{ "hooks": [{ "type": "command", "command": "matrix-claude", "args": ["hook"], "timeout": 2 }] }],
    "StopFailure": [{ "hooks": [{ "type": "command", "command": "matrix-claude", "args": ["hook"], "timeout": 2 }] }],
    "SessionEnd": [{ "hooks": [{ "type": "command", "command": "matrix-claude", "args": ["hook"], "timeout": 2 }] }]
  }
}
```

Replace the command **after `--`** with your existing executable and arguments.
If the original command uses shell syntax, invoke its shell explicitly, e.g.
`matrix-claude statusline -- sh -c 'your existing command'`. Nothing after `--`
is evaluated by the helper. Without a downstream executable it prints nothing.

On Windows, prefer a bare helper name on PATH to avoid differing PowerShell and
Git Bash rules for quoted executable paths. Use forward slashes and quote path
arguments with spaces. For an existing PowerShell renderer, the JSON command is:

```json
"command": "matrix-claude statusline -- powershell.exe -NoProfile -File \"C:/Users/Ada Example/.claude/statusline.ps1\""
```

The hook `command` can instead be an absolute `.exe` path, including spaces,
because its `args` array enables Claude's exec form (no shell quoting). The
downstream executable must also be real: `.bat`/`.cmd` scripts need an explicit
shell. Actual Claude/Git Bash/PowerShell activation still needs platform checks;
automated subprocess tests establish argv and byte preservation, not settings
interpretation by every installed Claude version.

Claude reloads status-line settings automatically. Review hooks with `/hooks`
and start a new session to exercise `SessionStart`. The status-line command
forwards stdin byte-for-byte, including trailing newlines and non-UTF8 bytes,
and inherits the downstream stdout/stderr and normal exit code. Invalid JSON or
metadata errors add no terminal output and never prevent forwarding. The helper
caps its metadata capture at 64 KiB while still forwarding larger input; it never
stores the original payload, prompts, transcript paths or text. Input forwarding
and downstream execution each have a two-second deadline; a stuck downstream is
terminated best-effort rather than holding the caller indefinitely. Descendant
processes started by a downstream shell are not independently supervised.
Metadata filesystem work has a 150 ms wait budget plus bounded runtime shutdown.

Hooks record start/working/idle/end using `hook_event_name`, ignoring child-agent
events. `SessionEnd` publishes an empty tombstone, so a late status callback
cannot resurrect it until the next `SessionStart`. Each session owns one atomic
snapshot and a small kernel-lock file; concurrent helpers never overwrite another
session. Lock contention retries for at most 50 ms, then skips optional metadata. Missed hook
edges cannot be reconstructed from status-line input; check `/hooks` if busy state
is wrong. Lock files and ended tombstones can remain on disk; readers expire them,
and they contain no conversation content.

`context_window.used_percentage` is the authoritative Claude percentage (rounded,
unknown stays null); unlike the OpenCode adapter, it follows Claude's input-only
definition. Set `refreshInterval: 15` to renew liveness during idle periods.
Status-line refresh changes heartbeat/context **only**, never activity order or
busy state. Without the refresh timer, quiet sessions intentionally expire after
60 seconds. Removing the wrapper/hooks restores your original setup; crashes or
missed SessionEnd also expire, rather than keeping a dead session forever. Feed
paths and Windows ACL caveats are the same as the OpenCode producer below.

Sources: [Claude status-line contract](https://code.claude.com/docs/en/statusline),
[hook events/exec form](https://code.claude.com/docs/en/hooks).

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

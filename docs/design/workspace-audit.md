# Signal Room workspace audit

Review question: can someone tell what is happening, what to do next, and
whether their action changes saved settings? The Overview review established
the visual direction: whitespace between tasks, one label per purpose, aligned
controls, accessible disclosure, and temporary action confirmations.

| Workspace | Finding | Change |
| --- | --- | --- |
| Configuration | A manual read preceded every first edit. The template/slot form duplicated Overview in different language. Both panels competed for attention, and Apply/Cancel meant the same thing as Save/Discard elsewhere. | Load on entry, keep the draft through navigation and reconnect, edit one side with the shared visual layout picker. Keep lock behavior, pinning, per-widget adjustments and revision recovery available on demand. Use Save changes and Discard changes throughout. |
| Devices | A port path looked like a panel identity. Routine enumeration and identification printed implementation details; a failed enumeration could appear as “no hardware.” | Scan on entry. Name panels by a unique location/assignment match only. Show readiness and Identify beside each panel; put firmware and location under Device details. Distinguish disconnected, failed, empty and detected states. Assignments remain an explicit draft edit. |
| Integrations | Separate lists forced users to mentally join subscription windows, feed status and sessions. Long IDs and timestamps dominated routine use. | Group by provider. Show usage and feed health independently, with aligned five-hour/weekly readings. Distinguish a real 0% from unavailable and stale. Disclose session IDs, activity, heartbeat and cache details when needed. |
| Diagnostics | Two top-level headings competed with the report. Raw capability explanations were given equal emphasis; “copied” could remain visible indefinitely. | Start with a machine check; show results as expandable status rows. Keep the redacted support report separate, with review and copy actions. Copy confirmation expires; the generated report remains available. |

All four use the same workspace heading, focus treatment, spacing and wrapping
action rows. Green denotes success or availability, muted labels neutral states,
and text names errors without relying on colour. Read-only checks do not start
identification, enable polling or save a draft.

## Review evidence

Rendered desktop frontend, illustrative two-panel data (no account/session IDs
from the owner's machine). Compare [Configuration](workspaces/configuration.png),
[Devices](workspaces/devices.png), [Integrations](workspaces/integrations.png)
and [Diagnostics](workspaces/diagnostics.png). The capture pass checks horizontal
overflow at 820×720 and 460×650. Native smoke coverage uses a separate Linux
Tauri process and isolated daemon; UI fixtures cover keyboard navigation,
reconnect, provider degradation, scan failure and report redaction.

On 2026-11-24, after restarting the rebuilt native desktop, the owner reported
“This looks much better.” This records positive design feedback, not release
sign-off or a walkthrough of every task. Screenshots do not establish physical
identification or clipboard behavior on every desktop.

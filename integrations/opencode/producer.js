// Optional, read-only V2 adapter. Never serialize SDK objects or error messages.
import { lstat, mkdir, open, rename, rm } from "node:fs/promises"
import { homedir } from "node:os"
import { join, posix, win32 } from "node:path"
import { randomUUID } from "node:crypto"

const safeID = (id) => typeof id === "string" && /^[A-Za-z0-9_-]{1,128}$/.test(id)
export function feedRoot(env = process.env, platform = process.platform, home = homedir()) {
  const path = platform === "win32" ? win32 : posix
  if (env.MATRIX_FEEDS) return env.MATRIX_FEEDS
  const base = platform === "win32"
    ? env.LOCALAPPDATA || path.join(home, "AppData", "Local")
    : env.XDG_RUNTIME_DIR || env.XDG_DATA_HOME || path.join(home, ".local", "share")
  return path.join(base, "fw16-matrix", "feeds")
}

export function contextPercent(message, models) {
  if (message?.type !== "assistant" || message.model?.providerID !== "openai") return null
  const limit = models.find((model) => model.providerID === "openai" && model.id === message.model.id)?.limit?.context
  const tokens = message.tokens
  if (!tokens || !Number.isFinite(limit) || limit <= 0) return null
  const values = [tokens.input, tokens.output, tokens.reasoning, tokens.cache?.read, tokens.cache?.write]
  if (values.some((value) => !Number.isFinite(value) || value < 0)) return null
  const percent = Math.round(100 * values.reduce((a, b) => a + b, 0) / limit)
  return percent <= 1000 ? percent : null
}

function timestamp(value, now) {
  return Number.isSafeInteger(value) && value >= 0 && value <= now + 5000 ? value : 0
}

async function privateDirectory(path) {
  await mkdir(path, { recursive: true, mode: 0o700 })
  const info = await lstat(path)
  if (!info.isDirectory() || info.isSymbolicLink()) throw new Error("unsafe metadata directory")
  if (process.platform !== "win32" && (info.uid !== process.getuid() || (info.mode & 0o077))) {
    throw new Error("metadata directory must be owner-private")
  }
}

export async function publish(root, feed) {
  if (!safeID(feed.instance_id)) throw new Error("invalid instance")
  await privateDirectory(root)
  const directory = join(root, "opencode")
  await privateDirectory(directory)
  const bytes = JSON.stringify(feed)
  if (Buffer.byteLength(bytes) > 65536 || feed.sessions.length > 16) throw new Error("metadata bounds")
  const path = join(directory, `${feed.instance_id}.json`)
  const temporary = join(directory, `${feed.instance_id}-${randomUUID()}.tmp`)
  let file
  try {
    file = await open(temporary, "wx", 0o600)
    await file.writeFile(bytes)
    await file.sync()
    await file.close(); file = undefined
    await rename(temporary, path)
  } finally {
    await file?.close()
    await rm(temporary, { force: true })
  }
}

// Polling uses public V2 read APIs, not V1 events or token totals on Session.Info.
// One job at a time: slow/failing SDK or filesystem calls never accumulate work.
export function start(ctx, options = {}) {
  const root = options.root ?? feedRoot()
  const instance = randomUUID()
  const now = options.now ?? Date.now
  const controller = new AbortController()
  let stopped = false
  let pending
  async function sample() {
    const request = { signal: AbortSignal.any([controller.signal, AbortSignal.timeout(5000)]) }
    const [page, active, inventory] = await Promise.all([
      ctx.session.list({ directory: ctx.location.directory, parentID: null, archived: false, limit: 16, order: "desc" }, request),
      ctx.session.active(request),
      ctx.model.list(undefined, request),
    ])
    const candidates = new Map()
    // Include old-but-running sessions even if they are outside the newest page.
    for (const id of Object.keys(active).filter(safeID).slice(0, 16)) {
      try { const session = await ctx.session.get({ sessionID: id }, request); candidates.set(id, session) } catch { /* isolated source */ }
    }
    for (const session of page.data.slice(0, 16)) if (!candidates.has(session.id)) candidates.set(session.id, session)
    const sessions = []
    const observed = now()
    for (const session of candidates.values()) {
      if (stopped) return
      if (!safeID(session.id) || session.parentID || session.time?.archived || session.location?.directory !== ctx.location.directory || session.model?.providerID !== "openai") continue
      try {
        const args = { sessionID: session.id, order: "desc", limit: 1 }
        const [response, user] = await Promise.all([
          ctx.session.message.list({ ...args, type: "assistant" }, request),
          ctx.session.message.list({ ...args, type: "user" }, request),
        ])
        const message = response.data[0]
        // Copy only scalar metadata; never retain text, titles, project paths,
        // tool arguments, credentials or cumulative session token counts.
        const times = [message?.time?.created, message?.time?.completed, message?.time?.streamed, user.data[0]?.time?.created]
        sessions.push({ session_id: session.id, parent_id: null,
          context_percent: contextPercent(message, inventory.data),
          busy: active[session.id]?.type === "running",
          last_activity_ms: Math.max(0, ...times.map((time) => timestamp(time, observed))),
        })
      } catch { /* Failed session is absent, not a fabricated idle/zero reading. */ }
    }
    sessions.sort((a, b) => b.last_activity_ms - a.last_activity_ms || a.session_id.localeCompare(b.session_id))
    if (!stopped) await publish(root, { schema_version: 1, instance_id: instance, provider: "opencode_openai", heartbeat_ms: observed, sessions: sessions.slice(0, 16) })
  }
  function tick() {
    if (stopped || pending) return pending ?? Promise.resolve()
    pending = sample().catch(() => {}).finally(() => { pending = undefined })
    return pending
  }
  const timer = (options.setInterval ?? setInterval)(() => void tick(), 5000)
  timer.unref?.()
  void tick()
  const dispose = async () => {
    stopped = true; controller.abort(); clearInterval(timer)
    await pending
    await rm(join(root, "opencode", `${instance}.json`), { force: true }).catch(() => {})
  }
  // Exposed only on the plain adapter for deterministic tests; V2 calls cleanup.
  dispose.tick = tick
  return dispose
}

// Metadata-only bridge to matrixd. Auto-loaded from OpenCode's plugins directory.
// No prompts, responses, tool arguments, credentials, or project paths hit disk.
import { mkdir, writeFile, rename, rm } from "node:fs/promises"
import { join } from "node:path"
import { randomUUID } from "node:crypto"

export default async function MatrixSession({ client, directory }) {
  const runtime = process.env.XDG_RUNTIME_DIR
  if (!runtime) return {}
  const root = join(runtime, "matrixd", "opencode")
  // Each OpenCode instance owns one file; disposing one cannot delete another's.
  const path = join(root, `${randomUUID()}.json`)
  const temporary = `${path}.tmp`
  const sessions = new Map()
  const parents = new Map()
  const models = new Map()
  const statuses = new Map()
  const userMessages = new Map()
  let stopped = false
  let lastWrite = 0
  let queue = Promise.resolve()

  // Preserve event order across asynchronous SDK/file calls. Errors in this
  // optional display must never reject a hook in the user's conversation.
  const enqueue = (fn) => {
    queue = queue.then(() => stopped ? undefined : fn()).catch(() => {})
    return queue
  }
  const options = () => ({ query: { directory }, signal: AbortSignal.timeout(2000) })

  async function isMain(id) {
    if (!id || typeof id !== "string") return false
    if (!parents.has(id)) {
      const { data } = await client.session.get({ ...options(), path: { id } })
      if (!data || data.id !== id) return false
      parents.set(id, Boolean(data.parentID))
    }
    return !parents.get(id)
  }

  async function observe(id, provider) {
    if (!await isMain(id)) return
    if (provider !== "openai") {
      sessions.delete(id)
      return
    }
    if (!sessions.has(id)) {
      sessions.set(id, {
        session_id: id,
        provider_id: "openai",
        context_pct: null,
        working: statuses.get(id) ?? false,
        updated_at: Date.now() / 1000,
      })
    }
    return sessions.get(id)
  }

  async function publish() {
    await mkdir(root, { recursive: true, mode: 0o700 })
    await writeFile(temporary, JSON.stringify({ version: 1, sessions: [...sessions.values()] }), { mode: 0o600 })
    await rename(temporary, path)
    lastWrite = Date.now()
  }

  async function contextPercent(info) {
    const key = `${info.providerID}/${info.modelID}`
    if (!models.has(key)) {
      const { data } = await client.provider.list(options())
      const model = data?.all?.find((p) => p.id === info.providerID)?.models?.[info.modelID]
      if (model?.limit?.context) models.set(key, model.limit.context)
    }
    const capacity = models.get(key)
    const tokens = info.tokens
    const parts = [tokens?.input, tokens?.output, tokens?.reasoning, tokens?.cache?.read, tokens?.cache?.write]
    if (!Number.isFinite(capacity) || capacity <= 0 || !parts.every((n) => Number.isFinite(n) && n >= 0)) return null
    // Match OpenCode's sidebar: latest assistant usage, not cumulative totals.
    return Math.min(100, Math.round(parts.reduce((a, b) => a + b, 0) / capacity * 100))
  }

  async function onEvent(event) {
    const p = event.properties ?? {}
    if (event.type === "session.created" || event.type === "session.updated") {
      if (p.info?.id) {
        parents.set(p.info.id, Boolean(p.info.parentID))
        if (p.info.parentID || p.info.time?.archived) {
          sessions.delete(p.info.id)
          await publish()
        }
      }
    } else if (event.type === "session.deleted") {
      sessions.delete(p.info?.id)
      parents.delete(p.info?.id)
      statuses.delete(p.info?.id)
      userMessages.delete(p.info?.id)
      await publish()
    } else if (event.type === "message.updated") {
      const info = p.info
      if (!info || !["user", "assistant"].includes(info.role)) return
      // OpenCode can update a user's summary/title later. That is not a new
      // prompt and must not pull selection away from another conversation.
      if (info.role === "user" && userMessages.get(info.sessionID)?.has(info.id)) return
      const provider = info.role === "user" ? info.model?.providerID : info.providerID
      const session = await observe(info.sessionID, provider)
      if (session) {
        if (info.role === "user") {
          if (!userMessages.has(info.sessionID)) userMessages.set(info.sessionID, new Set())
          userMessages.get(info.sessionID).add(info.id)
        }
        session.updated_at = Date.now() / 1000
        if (info.role === "assistant" && info.tokens?.output > 0) {
          session.context_pct = null
          session.context_pct = await contextPercent(info)
        }
      }
      await publish()
    } else if (event.type === "message.part.updated" || event.type === "message.part.delta") {
      const session = sessions.get(p.part?.sessionID ?? p.sessionID)
      if (session) {
        session.updated_at = Date.now() / 1000
        // Streaming can produce hundreds of events per second. The panel polls
        // at 1Hz, so coalesce these writes while still tracking latest activity.
        if (Date.now() - lastWrite >= 1000) await publish()
      }
    } else if (["session.status", "session.idle", "session.error"].includes(event.type)) {
      if (event.type === "session.status" && !["busy", "retry", "idle"].includes(p.status?.type)) return
      const working = event.type === "session.status" && ["busy", "retry"].includes(p.status.type)
      statuses.set(p.sessionID, working)
      const session = sessions.get(p.sessionID)
      if (session) {
        session.working = working
        // An idle/status refresh is not conversation activity.
        await publish()
      }
    }
  }

  // Only sessions observed in this running instance are exported. We do not
  // resurrect historical sessions from OpenCode's persistent database.
  const timer = setInterval(() => enqueue(publish), 15000)
  timer.unref?.()

  async function dispose() {
    stopped = true
    clearInterval(timer)
    await queue
    await Promise.all([rm(path, { force: true }), rm(temporary, { force: true })]).catch(() => {})
  }

  return {
    dispose,
    event: async ({ event }) => {
      if (event.type === "server.instance.disposed" && event.properties?.directory === directory) {
        await dispose()
        return
      }
      await enqueue(() => onEvent(event))
    },
    "chat.params": async ({ model }) => {
      // The actual selected model includes configured context-window overrides.
      // No SDK request is needed on the normal inference path.
      if (model?.providerID && model?.id) models.set(`${model.providerID}/${model.id}`, model.limit?.context)
    },
  }
}

import assert from "node:assert/strict"
import { test } from "node:test"
import { mkdtemp, readdir, readFile, rm, stat } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { spawnSync } from "node:child_process"
import MatrixSession from "../integration/opencode/matrix-session.js"

async function setup(t) {
  const runtime = await mkdtemp(join(tmpdir(), "matrix-plugin-"))
  const previous = process.env.XDG_RUNTIME_DIR
  process.env.XDG_RUNTIME_DIR = runtime
  const intervals = []
  t.mock.method(globalThis, "setInterval", (fn, delay) => {
    assert.equal(delay, 15000)
    intervals.push(fn)
    return { unref() {} }
  })
  t.mock.method(globalThis, "clearInterval", () => {})
  let now = Date.now()
  t.mock.method(Date, "now", () => now)
  const client = {
    session: { get: async ({ path }) => ({ data: { id: path.id, parentID: path.id === "child" ? "main" : undefined } }) },
    provider: { list: async () => ({ data: { all: [{ id: "openai", models: { model: { limit: { context: 1000 } } } }] } }) },
  }
  const instances = []
  const create = async () => {
    const hooks = await MatrixSession({ client, directory: "/project" })
    instances.push(hooks)
    return hooks
  }
  t.after(async () => {
    for (const hooks of instances) await hooks.dispose()
    if (previous === undefined) delete process.env.XDG_RUNTIME_DIR
    else process.env.XDG_RUNTIME_DIR = previous
    await rm(runtime, { recursive: true, force: true })
  })
  const root = join(runtime, "matrixd", "opencode")
  const snapshots = async () => Promise.all((await readdir(root)).filter((f) => f.endsWith(".json"))
    .map(async (f) => JSON.parse(await readFile(join(root, f), "utf8"))))
  const hooks = await create()
  return { hooks, create, client, intervals, snapshots, root, runtime, advance: (ms) => { now += ms } }
}

const send = (hooks, type, properties) => hooks.event({ event: { type, properties } })
const user = (sessionID = "main", providerID = "openai") => ({
  id: `user-${sessionID}`, sessionID, role: "user", model: { providerID, modelID: "model" },
})
const assistant = (sessionID = "main") => ({
  id: `assistant-${sessionID}`, sessionID, role: "assistant", providerID: "openai", modelID: "model",
  tokens: { input: 100, output: 50, reasoning: 10, cache: { read: 30, write: 10 } },
})

test("metadata bridge matches context calculation, activity, and the Python reader", async (t) => {
  const s = await setup(t)
  await send(s.hooks, "session.status", { sessionID: "main", status: { type: "busy" } })
  await send(s.hooks, "message.updated", { info: user() })
  const info = { ...assistant(), text: "PRIVATE RESPONSE", secret: "PRIVATE CREDENTIAL" }
  await send(s.hooks, "message.updated", { info })
  const [snapshot] = await s.snapshots()
  assert.deepEqual(snapshot.sessions[0], {
    session_id: "main", provider_id: "openai", context_pct: 20, working: true, updated_at: Date.now() / 1000,
  })
  const files = await readdir(s.root)
  assert.equal(files.length, 1)
  assert.equal((await stat(join(s.root, files[0]))).mode & 0o777, 0o600)
  const result = spawnSync("python3", ["-c", `
from matrixd.sources.opencode_session import read
session = read()
assert session is not None
assert session.session_id == 'main'
assert session.context_pct == 20
assert session.working
`], { env: process.env, encoding: "utf8", cwd: new URL("..", import.meta.url) })
  assert.equal(result.status, 0, result.stderr)
  await send(s.hooks, "session.status", { sessionID: "main", status: { type: "idle" } })
  assert.equal((await s.snapshots())[0].sessions[0].working, false)
})

test("configured model context overrides and zero-output messages", async (t) => {
  const s = await setup(t)
  await s.hooks["chat.params"]({ model: { providerID: "openai", id: "model", limit: { context: 500 } } })
  await send(s.hooks, "message.updated", { info: assistant() })
  assert.equal((await s.snapshots())[0].sessions[0].context_pct, 40)
  await send(s.hooks, "message.updated", { info: { ...assistant(), tokens: { ...assistant().tokens, output: 0 } } })
  assert.equal((await s.snapshots())[0].sessions[0].context_pct, 40)
})

test("subagents and conversations using other providers cannot take over", async (t) => {
  const s = await setup(t)
  await send(s.hooks, "message.updated", { info: assistant() })
  s.advance(1000)
  await send(s.hooks, "message.updated", { info: assistant("child") })
  await send(s.hooks, "message.updated", { info: user("other", "anthropic") })
  assert.deepEqual((await s.snapshots())[0].sessions.map((s) => s.session_id), ["main"])
  await send(s.hooks, "message.updated", { info: user("main", "anthropic") })
  assert.equal((await s.snapshots())[0].sessions.length, 0)
})

test("heartbeat preserves activity order; streaming updates it; idle edges do not", async (t) => {
  const s = await setup(t)
  await send(s.hooks, "message.updated", { info: assistant("old") })
  s.advance(2000)
  await send(s.hooks, "message.updated", { info: assistant("new") })
  const before = (await s.snapshots())[0].sessions
  s.advance(15000)
  await s.intervals[0]()
  await send(s.hooks, "session.status", { sessionID: "old", status: { type: "idle" } })
  assert.deepEqual((await s.snapshots())[0].sessions, before)
  s.advance(2000)
  await send(s.hooks, "message.part.delta", { sessionID: "old", delta: "PRIVATE TEXT" })
  const after = (await s.snapshots())[0].sessions
  assert.ok(after[0].updated_at > after[1].updated_at)
  assert.ok(!JSON.stringify(after).includes("PRIVATE TEXT"))
})

test("updating an old user message summary does not count as a new prompt", async (t) => {
  const s = await setup(t)
  await send(s.hooks, "message.updated", { info: user("old") })
  s.advance(2000)
  await send(s.hooks, "message.updated", { info: user("new") })
  const before = (await s.snapshots())[0].sessions
  s.advance(2000)
  await send(s.hooks, "message.updated", { info: { ...user("old"), summary: { title: "changed" } } })
  assert.deepEqual((await s.snapshots())[0].sessions, before)
})

test("retry is working; errors stop it; deletion and archive clear sessions", async (t) => {
  const s = await setup(t)
  await send(s.hooks, "message.updated", { info: assistant() })
  await send(s.hooks, "session.status", { sessionID: "main", status: { type: "retry" } })
  assert.equal((await s.snapshots())[0].sessions[0].working, true)
  await send(s.hooks, "session.error", { sessionID: "main" })
  assert.equal((await s.snapshots())[0].sessions[0].working, false)
  await send(s.hooks, "session.deleted", { info: { id: "main" } })
  assert.equal((await s.snapshots())[0].sessions.length, 0)
  await send(s.hooks, "message.updated", { info: assistant() })
  await send(s.hooks, "session.updated", { info: { id: "main", time: { archived: 100 } } })
  assert.equal((await s.snapshots())[0].sessions.length, 0)
})

test("instance disposal removes only its own snapshot and stops heartbeats", async (t) => {
  const s = await setup(t)
  const second = await s.create()
  await send(s.hooks, "message.updated", { info: assistant("one") })
  await send(second, "message.updated", { info: assistant("two") })
  assert.equal((await s.snapshots()).length, 2)
  await send(s.hooks, "server.instance.disposed", { directory: "/project" })
  await s.intervals[0]()
  assert.equal((await s.snapshots()).length, 1)
  assert.equal((await s.snapshots())[0].sessions[0].session_id, "two")
})

test("malformed events, unavailable SDK, and filesystem failure do not reject hooks", async (t) => {
  const s = await setup(t)
  s.client.session.get = async () => { throw new Error("unavailable") }
  await send(s.hooks, "message.updated", { info: assistant() })
  await send(s.hooks, "message.updated", {})
  await send(s.hooks, "session.status", { sessionID: "main" })
  await s.intervals[0]()
  assert.equal((await s.snapshots())[0].sessions.length, 0)
  // A non-directory runtime parent makes the next snapshot write fail.
  await rm(s.root, { recursive: true })
  const { writeFile } = await import("node:fs/promises")
  await writeFile(s.root, "blocked")
  await s.intervals[0]()
})

test("missing runtime directory makes integration inert", async () => {
  const previous = process.env.XDG_RUNTIME_DIR
  delete process.env.XDG_RUNTIME_DIR
  try {
    assert.deepEqual(await MatrixSession({ client: {} }), {})
  } finally {
    if (previous !== undefined) process.env.XDG_RUNTIME_DIR = previous
  }
})

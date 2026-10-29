import assert from "node:assert/strict"
import { test } from "node:test"
import { mkdtemp, readdir, readFile, rm, stat, symlink, chmod } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import plugin from "../index.js"
import { start, feedRoot, contextPercent, publish } from "../producer.js"

const model = { providerID: "openai", id: "test", limit: { context: 1000 } }
const assistant = { type: "assistant", model: { providerID: "openai", id: "test" }, time: { created: 90000, completed: 95000 }, tokens: { input: 200, output: 40, reasoning: 10, cache: { read: 100, write: 60 } }, content: [{ text: "never-export-this" }] }
async function fixture(t, overrides = {}) {
  const root = await mkdtemp(join(tmpdir(), "matrix-v2-"))
  t.after(() => rm(root, { recursive: true, force: true }))
  let time = 100000
  const session = (id = "ses_main") => ({ id, model: { providerID: "openai", id: "test" }, time: { updated: 99999 }, location: { directory: "/fixture" }, title: "never-export-this" })
  const ctx = { location: { directory: "/fixture" }, model: { list: async () => ({ data: [model] }) }, session: {
    list: async () => ({ data: [session()], cursor: {} }),
    active: async () => ({ ses_main: { type: "running" } }),
    get: async ({ sessionID }) => session(sessionID),
    message: { list: async ({ type }) => ({ data: type === "assistant" ? [assistant] : [{ type: "user", time: { created: 80000 }, text: "never-export-this" }] }) },
  } }
  overrides.configure?.(ctx, session)
  const cleanups = []
  t.after(async () => { for (const dispose of cleanups) await dispose() })
  const create = () => {
    const dispose = start(ctx, { root, now: () => time, setInterval: (_, delay) => { assert.equal(delay, 5000); return { unref() {} } } })
    cleanups.push(dispose); return dispose
  }
  async function feeds() {
    return Promise.all((await readdir(join(root, "opencode")).catch(() => [])).filter((name) => name.endsWith(".json")).map(async (name) => ({ name, ...(JSON.parse(await readFile(join(root, "opencode", name), "utf8"))) })))
  }
  return { ctx, create, feeds, root, setTime: (next) => { time = next } }
}

test("entrypoint is a V2 definition, not a V1 hook-returning function", () => {
  assert.equal(plugin.id, "fw16-matrix.metadata")
  assert.equal(typeof plugin.setup, "function")
})
test("platform paths match daemon defaults and explicit override", () => {
  assert.equal(feedRoot({ XDG_RUNTIME_DIR: "/run/user/1" }, "linux", "/home/a"), "/run/user/1/fw16-matrix/feeds")
  assert.equal(feedRoot({ XDG_DATA_HOME: "/data" }, "linux", "/home/a"), "/data/fw16-matrix/feeds")
  assert.equal(feedRoot({}, "linux", "/home/a"), "/home/a/.local/share/fw16-matrix/feeds")
  assert.equal(feedRoot({ LOCALAPPDATA: "C:\\Users\\A\\AppData\\Local" }, "win32"), "C:\\Users\\A\\AppData\\Local\\fw16-matrix\\feeds")
  assert.equal(feedRoot({ MATRIX_FEEDS: "/explicit" }), "/explicit")
})
test("latest response uses all five usage fields and actual model override", () => {
  assert.equal(contextPercent(assistant, [model]), 41)
  assert.equal(contextPercent(assistant, [{ ...model, limit: { context: 2000 } }]), 21)
  assert.equal(contextPercent({ ...assistant, tokens: undefined }, [model]), null)
  assert.equal(contextPercent(assistant, []), null)
  assert.equal(contextPercent({ ...assistant, tokens: { ...assistant.tokens, input: NaN } }, [model]), null)
})
test("publishes only bounded scalar schema metadata, not cumulative totals or text", async (t) => {
  const f = await fixture(t); const dispose = f.create(); await dispose.tick()
  const [feed] = await f.feeds()
  assert.equal(feed.schema_version, 1); assert.equal(feed.provider, "opencode_openai")
  assert.equal(feed.sessions[0].context_percent, 41); assert.equal(feed.sessions[0].busy, true)
  assert.equal(feed.sessions[0].last_activity_ms, 95000)
  assert.equal(JSON.stringify(feed).includes("never-export-this"), false)
  assert.equal(feed.name, `${feed.instance_id}.json`)
  if (process.platform !== "win32") assert.equal((await stat(join(f.root, "opencode", feed.name))).mode & 0o777, 0o600)
})
test("heartbeat never changes activity and idle comes from active drain status", async (t) => {
  const f = await fixture(t); const dispose = f.create(); await dispose.tick()
  f.ctx.session.active = async () => ({})
  f.setTime(120000); await dispose.tick()
  const [feed] = await f.feeds(); assert.equal(feed.heartbeat_ms, 120000)
  assert.equal(feed.sessions[0].last_activity_ms, 95000); assert.equal(feed.sessions[0].busy, false)
})
test("newer responses replace context rather than accumulating token usage", async (t) => {
  const f = await fixture(t); const dispose = f.create(); await dispose.tick()
  f.ctx.session.message.list = async ({ type }) => ({ data: type === "assistant" ? [{ ...assistant, tokens: { ...assistant.tokens, input: 0, output: 0, reasoning: 0, cache: { read: 100, write: 0 } } }] : [] })
  await dispose.tick(); assert.equal((await f.feeds())[0].sessions[0].context_percent, 10)
})
test("a failed session does not suppress healthy peers", async (t) => {
  const f = await fixture(t, { configure(ctx, session) {
    ctx.session.list = async () => ({ data: [session(), session("ses_broken")] })
    const list = ctx.session.message.list
    ctx.session.message.list = async (args) => { if (args.sessionID === "ses_broken") throw Error("secret"); return list(args) }
  } }); const dispose = f.create(); await dispose.tick()
  assert.deepEqual((await f.feeds())[0].sessions.map((s) => s.session_id), ["ses_main"])
})
test("excludes children, other providers and unrelated locations", async (t) => {
  const f = await fixture(t, { configure(ctx, session) {
    ctx.session.active = async () => ({})
    ctx.session.list = async () => ({ data: [{ ...session("ses_child"), parentID: "ses_parent" }, { ...session("ses_other"), model: { providerID: "anthropic" } }, { ...session("ses_away"), location: { directory: "/other" } }] })
  } }); const dispose = f.create(); await dispose.tick(); assert.deepEqual((await f.feeds())[0].sessions, [])
})
test("instance cleanup cannot delete another instance and leaves no temporary files", async (t) => {
  const f = await fixture(t); const first = f.create(); const second = f.create()
  await Promise.all([first.tick(), second.tick()]); assert.equal((await f.feeds()).length, 2)
  await first(); assert.equal((await f.feeds()).length, 1)
  await second(); assert.deepEqual(await readdir(join(f.root, "opencode")), [])
})
test("SDK failure is isolated and does not renew stale heartbeat; recovery republishes", async (t) => {
  const f = await fixture(t); const dispose = f.create(); await dispose.tick()
  const previous = f.ctx.model.list; f.ctx.model.list = async () => { throw Error("secret") }
  f.setTime(200000); await dispose.tick(); assert.equal((await f.feeds())[0].heartbeat_ms, 100000)
  f.ctx.model.list = previous; await dispose.tick(); assert.equal((await f.feeds())[0].heartbeat_ms, 200000)
})
test("concurrent ticks coalesce rather than queuing SDK calls", async (t) => {
  let release; let calls = 0
  const wait = new Promise((resolve) => { release = resolve })
  const f = await fixture(t, { configure(ctx) { ctx.model.list = async () => { calls++; await wait; return { data: [model] } } } })
  const dispose = f.create(); const ticks = Array.from({ length: 30 }, () => dispose.tick())
  release(); await Promise.all(ticks); assert.equal(calls, 1)
})
test("reader bounds enforced even with excessive session inventory", async (t) => {
  const f = await fixture(t, { configure(ctx, session) {
    ctx.session.active = async () => ({})
    ctx.session.list = async () => ({ data: Array.from({ length: 40 }, (_, i) => session(`ses_${i}`)) })
  } }); const dispose = f.create(); await dispose.tick(); assert.equal((await f.feeds())[0].sessions.length, 16)
})
test("unsafe metadata directories fail closed without affecting client", async (t) => {
  const f = await fixture(t); const blocked = join(f.root, "blocked")
  if (process.platform !== "win32") {
    await chmod(f.root, 0o755)
    await assert.rejects(publish(f.root, { instance_id: "safe", sessions: [] }))
    await chmod(f.root, 0o700)
    await symlink(f.root, blocked)
    await assert.rejects(publish(blocked, { instance_id: "safe", sessions: [] }))
  }
  await assert.rejects(publish(f.root, { instance_id: "../escape", sessions: [] }))
})

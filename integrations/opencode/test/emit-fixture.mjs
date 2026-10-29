// Crash-style fixture: use the public producer, leave its snapshot for Rust.
import { start } from "../producer.js"
const model = { providerID: "openai", id: "fixture", limit: { context: 1000 } }
const session = { id: "ses_contract", model, time: {}, location: { directory: "/fixture" } }
const dispose = start({
  location: { directory: "/fixture" },
  model: { list: async () => ({ data: [model] }) },
  session: {
    list: async () => ({ data: [session] }), active: async () => ({}), get: async () => session,
    message: { list: async ({ type }) => ({ data: type === "assistant" ? [{ type, model, time: { created: 90000 }, tokens: { input: 200, output: 40, reasoning: 10, cache: { read: 100, write: 60 } } }] : [] }) },
  },
}, { root: process.argv[2], now: () => 100000, setInterval: () => ({ unref() {} }) })
await dispose.tick()

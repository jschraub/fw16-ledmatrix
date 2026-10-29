import { Plugin } from "@opencode/plugin"
import { start } from "./producer.js"

export default Plugin.define({
  id: "fw16-matrix.metadata",
  setup: start,
})

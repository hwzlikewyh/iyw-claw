import { spawn } from "node:child_process"
import { createInterface } from "node:readline"

const MAX_FRAME_BYTES = 64 * 1024 * 1024

export class PluginRpc {
  pending = new Map()
  sequence = 0

  constructor({ entrypoint, cwd, env, timeout = 180_000 }) {
    this.timeout = timeout
    this.child = spawn(process.execPath, [entrypoint], {
      cwd,
      env: { ...captureEnvironment(), ...env },
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    })
    // 插件的 stderr 可能含登录信息，不转发到导入日志。
    this.child.stderr.resume()
    this.child.on("error", (error) => this.fail(error))
    this.child.on("exit", (code) => this.fail(Error(`MCP exited (${code})`)))
    this.lines = createInterface({ input: this.child.stdout })
    this.lines.on("line", (line) => this.receive(line))
    let bufferedBytes = 0
    this.child.stdout.on("data", (chunk) => {
      const lastNewline = chunk.lastIndexOf(10)
      bufferedBytes =
        lastNewline < 0
          ? bufferedBytes + chunk.length
          : chunk.length - lastNewline - 1
      if (bufferedBytes > MAX_FRAME_BYTES)
        this.fail(Error("MCP frame is too large"))
    })
  }

  receive(line) {
    if (Buffer.byteLength(line) > MAX_FRAME_BYTES)
      return this.fail(Error("MCP frame is too large"))
    let message
    try {
      message = JSON.parse(line)
    } catch {
      return this.fail(Error("Invalid MCP JSON"))
    }
    if (message.method && message.id !== undefined) {
      this.send({
        id: message.id,
        ...(message.method === "ping"
          ? { result: {} }
          : { error: { code: -32601, message: "Unsupported client request" } }),
      })
      return
    }
    const pending = this.pending.get(message.id)
    if (!pending) return
    this.pending.delete(message.id)
    clearTimeout(pending.timer)
    if (message.error)
      pending.reject(
        Error(`MCP error ${message.error.code}: ${message.error.message}`)
      )
    else pending.resolve(message.result)
  }

  send(value) {
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", ...value })}\n`)
  }

  call(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++this.sequence
      const timer = setTimeout(() => {
        this.pending.delete(id)
        reject(Error(`MCP ${method} timed out`))
      }, this.timeout)
      this.pending.set(id, { resolve, reject, timer })
      this.send({ id, method, params })
    })
  }

  async initialize() {
    const result = await this.call("initialize", {
      protocolVersion: "2025-11-25",
      capabilities: {
        extensions: {
          "io.modelcontextprotocol/ui": {
            mimeTypes: ["text/html;profile=mcp-app"],
          },
        },
      },
      clientInfo: { name: "iyw-native-plugin-import", version: "1.0.0" },
    })
    this.send({ method: "notifications/initialized" })
    return result
  }

  async list(method, key) {
    const items = []
    let cursor
    const seen = new Set()
    do {
      const page = await this.call(method, cursor ? { cursor } : {})
      if (!Array.isArray(page[key])) throw Error(`Invalid MCP ${method} result`)
      items.push(...page[key])
      cursor = page.nextCursor
      if (cursor && seen.has(cursor))
        throw Error("MCP pagination repeats a cursor")
      if (items.length > 512) throw Error("MCP inventory exceeds 512 entries")
      seen.add(cursor)
    } while (cursor)
    return items
  }

  fail(error) {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer)
      pending.reject(error)
    }
    this.pending.clear()
    this.lines?.close()
    this.child.kill()
  }

  async close() {
    if (!this.child.stdin.destroyed && !this.child.stdin.writableEnded) {
      this.child.stdin.end()
    }
    const timer = setTimeout(() => this.child.kill(), 2_000)
    if (this.child.exitCode === null && this.child.signalCode === null) {
      await new Promise((resolve) => this.child.once("exit", resolve))
    }
    clearTimeout(timer)
    this.lines.close()
  }
}

function captureEnvironment() {
  const names = ["SystemRoot", "windir", "ComSpec", "PATH", "Path", "PATHEXT", "TEMP", "TMP", "HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "XDG_CACHE_HOME", "XDG_CONFIG_HOME", "IYW_WORKSPACE_DIR", "IYW_PLUGIN_DATA_DIR"]
  return Object.fromEntries(names.flatMap((key) => process.env[key] === undefined ? [] : [[key, process.env[key]]]))
}

import { spawn } from "node:child_process"
import { join } from "node:path"

const MAX_OUTPUT_BYTES = 2 * 1024 * 1024
const COMMAND_TIMEOUT_MS = 60_000
const children = new Set()

function kill(child) {
  if (child.exitCode !== null || !child.pid) return
  if (process.platform === "win32") {
    spawn(
      join(process.env.SystemRoot ?? "C:\\Windows", "System32/taskkill.exe"),
      ["/pid", String(child.pid), "/t", "/f"],
      { windowsHide: true, stdio: "ignore" }
    )
  } else {
    try {
      process.kill(-child.pid, "SIGTERM")
    } catch {
      child.kill()
    }
  }
}

export function stopChildren() {
  for (const child of children) kill(child)
}

function capture(child, inherit) {
  const output = { stdout: "", stderr: "", error: null }
  if (inherit) return output
  let size = 0
  for (const stream of ["stdout", "stderr"]) {
    child[stream].setEncoding("utf8")
    child[stream].on("data", (text) => {
      size += Buffer.byteLength(text)
      if (size > MAX_OUTPUT_BYTES) {
        output.error = new Error("连接器返回内容过大，请缩小操作范围")
        kill(child)
      } else output[stream] += text
    })
  }
  return output
}

export function execute(context, command, options = {}) {
  const { inherit = false, timeoutMs = COMMAND_TIMEOUT_MS, signal } = options
  if (signal?.aborted) return Promise.reject(new Error("连接器操作已取消"))
  const child = spawn(command.program, command.args, {
    cwd: context.account,
    env: context.env,
    shell: false,
    windowsHide: true,
    detached: process.platform !== "win32",
    stdio: inherit ? "inherit" : ["ignore", "pipe", "pipe"],
  })
  children.add(child)
  const output = capture(child, inherit)
  const cancel = () => {
    output.error = new Error("连接器操作已取消")
    kill(child)
  }
  signal?.addEventListener("abort", cancel, { once: true })
  const timer =
    timeoutMs === null
      ? null
      : setTimeout(() => {
          output.error = new Error("连接器命令执行超时")
          kill(child)
        }, timeoutMs)
  return new Promise((resolve, reject) => {
    child.once("error", reject)
    child.once("close", (code) =>
      output.error
        ? reject(output.error)
        : resolve({ code: code ?? 1, ...output })
    )
  }).finally(() => {
    if (timer) clearTimeout(timer)
    signal?.removeEventListener("abort", cancel)
    children.delete(child)
  })
}

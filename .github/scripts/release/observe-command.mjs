import { execFileSync, spawn } from "node:child_process"
import { freemem, totalmem } from "node:os"

const SAMPLE_MS = 60_000
const SAMPLE_TIMEOUT_MS = 5_000
const TOP_PROCESSES = 6
const BYTES_PER_MIB = 1024 * 1024

function sample(label) {
  const memory = { totalMiB: Math.round(totalmem() / BYTES_PER_MIB), freeMiB: Math.round(freemem() / BYTES_PER_MIB) }
  console.log(`[build-resources] ${new Date().toISOString()} ${label} ${JSON.stringify(memory)}`)
  if (process.platform === "win32") return
  try {
    // 不记录命令参数，避免将签名参数或凭据带入日志。
    const output = execFileSync("ps", ["-axo", "pid,ppid,pcpu,rss,comm"], {
      encoding: "utf8", timeout: SAMPLE_TIMEOUT_MS,
    })
    const rows = output.trim().split("\n").slice(1)
    rows.sort((a, b) => Number(b.trim().split(/\s+/)[3]) - Number(a.trim().split(/\s+/)[3]))
    console.log(rows.slice(0, TOP_PROCESSES).join("\n"))
  } catch (error) {
    console.warn(`[build-resources] process sample unavailable: ${error.code || error.status}`)
  }
}

export async function runObserved(command, args, { label }) {
  const started = Date.now()
  console.log(`[build-phase] starting ${label}`)
  sample(label)
  const child = spawn(command, args, { stdio: "inherit", windowsHide: true })
  const timer = setInterval(() => sample(label), SAMPLE_MS)
  timer.unref()
  try {
    await new Promise((resolve, reject) => {
      child.once("error", reject)
      child.once("exit", (code, signal) => code === 0 ? resolve() :
        reject(new Error(`${label} failed: exit=${code}, signal=${signal || "none"}`)))
    })
  } finally {
    clearInterval(timer)
    console.log(`[build-phase] finished ${label}; elapsed_seconds=${((Date.now() - started) / 1000).toFixed(1)}`)
  }
}

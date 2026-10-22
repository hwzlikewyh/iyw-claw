import { spawn } from "node:child_process"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"

const scriptDir = dirname(fileURLToPath(import.meta.url))
const root = dirname(scriptDir)
const fusionBaseUrl =
  process.env.IYW_CLAW_FUSION_API_BASE_URL ??
  "https://gateway.iyw.cn/iyw-fusion-api"

const child = spawn(
  "cargo",
  ["run", "--bin", "iyw-claw-server", "--no-default-features", "--features", "server-runtime"],
  {
    cwd: join(root, "src-tauri"),
    env: { ...process.env, IYW_CLAW_FUSION_API_BASE_URL: fusionBaseUrl },
    stdio: "inherit",
  }
)

const forwardSignal = (signal) => {
  if (!child.killed) child.kill(signal)
}

process.on("SIGINT", () => forwardSignal("SIGINT"))
process.on("SIGTERM", () => forwardSignal("SIGTERM"))

child.on("error", (error) => {
  console.error(`[server:dev] failed to start cargo: ${error.message}`)
  process.exitCode = 1
})

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal)
    return
  }
  process.exitCode = code ?? 1
})

import { spawnSync } from "node:child_process"
import { fileURLToPath } from "node:url"
import {
  windowsBuildEnvironment,
  isWin7Target,
  WIN7_TARGET,
} from "./build-desktop-windows.mjs"

if (process.platform !== "win32")
  throw new Error("Win7 packaging requires a Windows build machine")

const target = process.env.TAURI_TARGET_TRIPLE || WIN7_TARGET
if (!isWin7Target(target)) throw new Error(`Unsupported Win7 target: ${target}`)

const result = spawnSync(
  process.execPath,
  [
    fileURLToPath(new URL("./build-desktop.mjs", import.meta.url)),
    ...process.argv.slice(2),
  ],
  {
    cwd: fileURLToPath(new URL("../..", import.meta.url)),
    env: windowsBuildEnvironment(target),
    stdio: "inherit",
    windowsHide: true,
  }
)
if (result.error) throw result.error
process.exitCode = result.status ?? 1

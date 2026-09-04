#!/usr/bin/env node

// 在已构建的桌面目标上验证 agent-browser 与受管引擎的最小生命周期。
// 该脚本不下载浏览器，也不连接用户 profile；CI 通过 fixture 显式提供路径。

import { execFileSync } from "node:child_process"
import { existsSync, mkdirSync, rmSync } from "node:fs"
import { join, resolve } from "node:path"
import { tmpdir } from "node:os"
import process from "node:process"

const VERSION = "0.36.0"

function parseArgs(argv) {
  const values = {
    sidecar: process.env.IYW_CLAW_BROWSER_SIDECAR ?? "",
    engine: process.env.IYW_CLAW_BROWSER_ENGINE ?? "",
  }
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    if (arg === "--sidecar" || arg === "--engine") {
      const value = argv[++index]
      if (!value || value.startsWith("--"))
        throw new Error(`missing value for ${arg}`)
      values[arg.slice(2)] = value
    } else {
      throw new Error(`unknown argument: ${arg}`)
    }
  }
  return values
}

function requiredSmoke() {
  return ["1", "true", "yes"].includes(
    String(process.env.IYW_CLAW_REQUIRED_BROWSER_SMOKE ?? "").toLowerCase()
  )
}

function failOrSkip(message, required) {
  if (required) throw new Error(message)
  console.log(`[browser-smoke] skipped: ${message}`)
  process.exit(0)
}

function runCommand(sidecar, args, env, label) {
  let output
  try {
    output = execFileSync(
      sidecar,
      ["--session", "iyw-smoke", "--json", ...args],
      {
        env,
        encoding: "utf8",
        timeout: 30_000,
        windowsHide: true,
        stdio: ["ignore", "pipe", "pipe"],
      }
    )
  } catch (error) {
    throw new Error(`${label} failed with exit ${error.status ?? "unknown"}`)
  }
  let payload
  try {
    payload = JSON.parse(output)
  } catch {
    throw new Error(`${label} returned invalid JSON`)
  }
  if (payload.success !== true)
    throw new Error(`${label} returned unsuccessful result`)
  return payload
}

function assertPath(path, label) {
  if (!existsSync(path)) throw new Error(`${label} fixture is missing`)
  return resolve(path)
}

function main() {
  const required = requiredSmoke()
  const { sidecar, engine } = parseArgs(process.argv.slice(2))
  if (!sidecar || !engine)
    failOrSkip("sidecar and engine fixtures are required", required)
  const sidecarPath = assertPath(sidecar, "sidecar")
  const enginePath = assertPath(engine, "browser engine")
  const root = join(tmpdir(), `iyw-claw-browser-smoke-${process.pid}`)
  const environment = {
    ...process.env,
    AGENT_BROWSER_SOCKET_DIR: join(root, "sockets"),
    AGENT_BROWSER_PROFILE: join(root, "profile"),
    AGENT_BROWSER_EXECUTABLE_PATH: enginePath,
    AGENT_BROWSER_DOWNLOAD_PATH: join(root, "downloads"),
    AGENT_BROWSER_SCREENSHOT_DIR: join(root, "screenshots"),
    AGENT_BROWSER_IDLE_TIMEOUT_MS: "0",
    AGENT_BROWSER_HEADED: "0",
    AGENT_BROWSER_NO_AUTO_DIALOG: "1",
  }
  for (const directory of [
    environment.AGENT_BROWSER_SOCKET_DIR,
    environment.AGENT_BROWSER_PROFILE,
    environment.AGENT_BROWSER_DOWNLOAD_PATH,
    environment.AGENT_BROWSER_SCREENSHOT_DIR,
  ]) {
    mkdirSync(directory, { recursive: true })
  }
  try {
    runCommand(sidecarPath, ["open", "about:blank"], environment, "open")
    const cdp = runCommand(
      sidecarPath,
      ["get", "cdp-url"],
      environment,
      "cdp-url"
    )
    const cdpUrl = cdp.data?.cdpUrl
    if (
      typeof cdpUrl !== "string" ||
      !/^ws:\/\/(127\.0\.0\.1|localhost|\[::1\]):/.test(cdpUrl)
    ) {
      throw new Error("cdp-url did not return a loopback websocket")
    }
    const snapshot = runCommand(
      sidecarPath,
      ["snapshot"],
      environment,
      "snapshot"
    )
    if (typeof snapshot.data?.snapshot !== "string")
      throw new Error("snapshot payload is missing")
    runCommand(sidecarPath, ["stream", "enable"], environment, "stream enable")
    runCommand(sidecarPath, ["stream", "status"], environment, "stream status")
    runCommand(sidecarPath, ["screenshot"], environment, "screenshot")
    console.log(`[browser-smoke] agent-browser ${VERSION} lifecycle passed`)
  } finally {
    try {
      runCommand(sidecarPath, ["close"], environment, "close")
    } catch (error) {
      console.error(`[browser-smoke] close failed: ${error.message}`)
    }
    rmSync(root, { recursive: true, force: true })
  }
}

try {
  main()
} catch (error) {
  console.error(`[browser-smoke] ${error.message}`)
  process.exit(1)
}

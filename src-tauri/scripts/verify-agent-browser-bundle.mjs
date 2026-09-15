import { readFileSync } from "node:fs"
import { join } from "node:path"

export const AGENT_BROWSER_NAME = "agent-browser"
const SUPPORTED_TARGETS = new Set([
  "x86_64-pc-windows-msvc",
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
])
const EXCLUDED_TARGET = "i686-pc-windows-msvc"
const VERSION = "0.37.1"
const EXPECTED = {
  "x86_64-pc-windows-msvc": [
    13942272,
    "29a003139ff4eb96fa4d1ed341830b26eb3e082843bf776b4e88ad3443bb8fde",
  ],
  "x86_64-apple-darwin": [
    13592392,
    "c79d1e0525c0bf79df9eec355269ae40bcda9c4a3fce3f242c24faecaaaeef84",
  ],
  "aarch64-apple-darwin": [
    12429376,
    "e52f06476ea0f1d14357c1924ce1d7f1bf08279f2642d74ccfa7ee935c46aea1",
  ],
  "x86_64-unknown-linux-gnu": [
    14253840,
    "f8e5f9294bd0da70dda61854f12004fd61c668cd682bfb600cdf6d0df73dea69",
  ],
  "aarch64-unknown-linux-gnu": [
    12507648,
    "d54d3e1262dc1aa0906e0677adc6d0cbb40d1274631f4cf77136bf23a0bc20e9",
  ],
}

export function agentBrowserStagePath(srcTauri, target) {
  return join(
    srcTauri,
    "binaries",
    `${AGENT_BROWSER_NAME}-${target}${target.includes("windows") ? ".exe" : ""}`
  )
}

export function verifyAgentBrowserConfig(srcTauri, target, die) {
  if (target === EXCLUDED_TARGET) {
    const config = JSON.parse(
      readFileSync(join(srcTauri, "tauri.windows-x86.conf.json"), "utf8")
    )
    if (config.bundle?.externalBin?.length)
      die("Windows x86 must exclude agent-browser")
    return
  }
  if (!SUPPORTED_TARGETS.has(target)) return
  const configName = target.includes("windows")
    ? "tauri.windows.conf.json"
    : "tauri.conf.json"
  const config = JSON.parse(readFileSync(join(srcTauri, configName), "utf8"))
  const configured = config.bundle?.externalBin?.includes(
    "binaries/agent-browser"
  )
  if (configured !== true) {
    die(`${configName} agent-browser declaration does not match ${target}`)
  }
}

export function verifyStagedAgentBrowser(srcTauri, target, tools) {
  if (!SUPPORTED_TARGETS.has(target)) return
  const path = agentBrowserStagePath(srcTauri, target)
  const stats = tools.logFile("Tauri agent-browser sidecar", path, VERSION)
  assertPinned(path, stats, tools, target)
}

export function verifyInstalledAgentBrowser(
  appDirectory,
  target,
  expectedHashes,
  tools
) {
  if (!SUPPORTED_TARGETS.has(target)) return
  const path = join(
    appDirectory,
    `${AGENT_BROWSER_NAME}${target.includes("windows") ? ".exe" : ""}`
  )
  const stats = tools.logFile("installed agent-browser sidecar", path, VERSION)
  const digest = assertPinned(path, stats, tools, target)
  if (expectedHashes && digest !== expectedHashes.get(AGENT_BROWSER_NAME)) {
    tools.die(`installed agent-browser differs from staged source: ${path}`)
  }
}

export function addAgentBrowserHash(hashes, srcTauri, target, sha256) {
  if (SUPPORTED_TARGETS.has(target)) {
    hashes.set(
      AGENT_BROWSER_NAME,
      sha256(agentBrowserStagePath(srcTauri, target))
    )
  }
}

function assertPinned(path, stats, tools, target) {
  const [expectedSize, expectedHash] = EXPECTED[target] ?? []
  const digest = tools.sha256(path)
  if (stats.size !== expectedSize || digest !== expectedHash) {
    tools.die(`agent-browser differs from pinned v${VERSION}: ${path}`)
  }
  return digest
}

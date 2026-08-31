import { readFileSync } from "node:fs"
import { join } from "node:path"

export const AGENT_BROWSER_NAME = "agent-browser"
const SUPPORTED_TARGET = "x86_64-pc-windows-msvc"
const EXCLUDED_TARGET = "i686-pc-windows-msvc"
const VERSION = "0.35.1"
const EXPECTED_SIZE = 13_665_280
const EXPECTED_SHA256 =
  "def2614c2c193518463ad9126718a1ff828a7bf217d7f75f156249c0dbb16c83"

export function agentBrowserStagePath(srcTauri, target) {
  return join(srcTauri, "binaries", `${AGENT_BROWSER_NAME}-${target}.exe`)
}

export function verifyAgentBrowserConfig(srcTauri, target, die) {
  if (target !== SUPPORTED_TARGET && target !== EXCLUDED_TARGET) return
  const configName =
    target === SUPPORTED_TARGET
      ? "tauri.windows.conf.json"
      : "tauri.windows-x86.conf.json"
  const config = JSON.parse(readFileSync(join(srcTauri, configName), "utf8"))
  const configured = config.bundle?.externalBin?.includes(
    "binaries/agent-browser"
  )
  if (configured !== (target === SUPPORTED_TARGET)) {
    die(`${configName} agent-browser declaration does not match ${target}`)
  }
}

export function verifyStagedAgentBrowser(srcTauri, target, tools) {
  if (target !== SUPPORTED_TARGET) return
  const path = agentBrowserStagePath(srcTauri, target)
  const stats = tools.logFile("Tauri agent-browser sidecar", path, VERSION)
  assertPinned(path, stats, tools)
}

export function verifyInstalledAgentBrowser(
  appDirectory,
  target,
  expectedHashes,
  tools
) {
  if (target !== SUPPORTED_TARGET) return
  const path = join(appDirectory, `${AGENT_BROWSER_NAME}.exe`)
  const stats = tools.logFile("installed agent-browser sidecar", path, VERSION)
  const digest = assertPinned(path, stats, tools)
  if (expectedHashes && digest !== expectedHashes.get(AGENT_BROWSER_NAME)) {
    tools.die(`installed agent-browser differs from staged source: ${path}`)
  }
}

export function addAgentBrowserHash(hashes, srcTauri, target, sha256) {
  if (target === SUPPORTED_TARGET) {
    hashes.set(
      AGENT_BROWSER_NAME,
      sha256(agentBrowserStagePath(srcTauri, target))
    )
  }
}

function assertPinned(path, stats, tools) {
  const digest = tools.sha256(path)
  if (stats.size !== EXPECTED_SIZE || digest !== EXPECTED_SHA256) {
    tools.die(`agent-browser differs from pinned v${VERSION}: ${path}`)
  }
  return digest
}

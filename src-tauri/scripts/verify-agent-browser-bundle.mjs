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
const VERSION = "0.35.2"
const EXPECTED = {
  "x86_64-pc-windows-msvc": [13707264, "5ffcad90cda06114730e8b202285c45ec0866d1b8d7876b561329e4a8cfbb126"],
  "x86_64-apple-darwin": [13378880, "d76cfc76885d5007f3c119008a80a145b381ec4dfdd202f43e46cd0829751774"],
  "aarch64-apple-darwin": [12247424, "e1e08f3b0a1c711750209e6a25b6f3a9dab7ed6e6a24b55a2556050b991fcc97"],
  "x86_64-unknown-linux-gnu": [14021032, "b699f24eebdb7fde91a34a9d697a1b84c3145f54327b60694b46f06b2972ce4d"],
  "aarch64-unknown-linux-gnu": [12332896, "1599fec4f4e75dc26fc08eecc06ca4b729a0361932b32a6afb99885f0f829ecb"],
}

export function agentBrowserStagePath(srcTauri, target) {
  return join(srcTauri, "binaries", `${AGENT_BROWSER_NAME}-${target}${target.includes("windows") ? ".exe" : ""}`)
}

export function verifyAgentBrowserConfig(srcTauri, target, die) {
  if (target === EXCLUDED_TARGET) {
    const config = JSON.parse(readFileSync(join(srcTauri, "tauri.windows-x86.conf.json"), "utf8"))
    if (config.bundle?.externalBin?.length) die("Windows x86 must exclude agent-browser")
    return
  }
  if (!SUPPORTED_TARGETS.has(target)) return
  const configName = target.includes("windows") ? "tauri.windows.conf.json" : "tauri.conf.json"
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
  const path = join(appDirectory, `${AGENT_BROWSER_NAME}${target.includes("windows") ? ".exe" : ""}`)
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

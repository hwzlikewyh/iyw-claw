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
const VERSION = "0.36.0"
const EXPECTED = {
  "x86_64-pc-windows-msvc": [
    13837312,
    "412ff72737a109e93f5304b0ff76c988fb6f1f451d0fc7e010577922bcc20ff3",
  ],
  "x86_64-apple-darwin": [
    13510280,
    "45d9ac061a7d72e61eaff905326e2e19365f4dadb12142ea2f2d76d84689c708",
  ],
  "aarch64-apple-darwin": [
    12363200,
    "b2106ab39db0838e7b1772f7f26f760518de56d09053150c56f9dddf15af997d",
  ],
  "x86_64-unknown-linux-gnu": [
    14156776,
    "56d15181e51e00213f907fcf39707cfc76bfa804ff20f5a9373661c73f96de5e",
  ],
  "aarch64-unknown-linux-gnu": [
    12442720,
    "aeb556addca3903601a433de1acad3ace1c9c61d170084bf58d875884599a990",
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

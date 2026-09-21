#!/usr/bin/env node

import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

const version =
  process.argv[2] ||
  JSON.parse(
    readFileSync(new URL("../../package.json", import.meta.url), "utf8")
  ).version
const platforms = [
  ["windows", "x86_64"],
  ["darwin", "x86_64"],
  ["darwin", "aarch64"],
  ["linux", "x86_64"],
  ["linux", "aarch64"],
]
const required = ["node", "git", "uv", "chromix", "agent-browser"]
const allowed = new Set([
  ...required,
  "officecli",
  "agent-reach",
  "open-computer-use",
  "environment-maintainer",
])
const endpoint = new URL(
  `${(
    process.env.IYW_CLAW_FUSION_API_BASE_URL ||
    "https://gateway.iyw.cn/iyw-fusion-api"
  ).replace(/\/$/, "")}/app-updates/v1/environment/resolve`
)
assert.equal(endpoint.protocol, "https:")
assert.ok(!endpoint.username && !endpoint.password)

function validatePlan(plan, target, arch) {
  assert.equal(plan.pcVersion, version)
  assert.equal(plan.target, target)
  assert.equal(plan.arch, arch)
  assert.ok(plan.catalogRevision > 0 && plan.bindingRevision > 0)
  assert.ok(Array.isArray(plan.actions))
  const seen = new Set()
  for (const action of plan.actions) {
    assert.ok(
      allowed.has(action.componentId),
      "Unsupported environment component"
    )
    assert.ok(!seen.has(action.componentId), "Duplicate environment component")
    seen.add(action.componentId)
    assert.equal(action.action, "install")
    assert.ok(typeof action.version === "string" && action.version.length > 0)
    assert.ok(action.artifact.size > 0)
    assert.match(action.artifact.sha256, /^[a-f0-9]{64}$/)
    assert.match(action.artifact.versionId, /^[1-9][0-9]*$/)
    assert.match(action.artifact.artifactId, /^[1-9][0-9]*$/)
    const url = new URL(action.artifact.url)
    assert.equal(url.protocol, "https:")
    assert.ok(!url.username && !url.password && !url.port && !url.hash)
    assert.ok(
      url.hostname === "vol-ai.iywtu.com" ||
        (url.hostname.includes(".tos-") && url.hostname.endsWith(".volces.com"))
    )
  }
  for (const id of required)
    assert.ok(seen.has(id), `Missing required component: ${id}`)
  return [...seen].join(", ")
}

async function verifyPlatform(target, arch) {
  const response = await fetch(endpoint, {
    method: "POST",
    redirect: "error",
    headers: { "content-type": "application/json" },
    signal: AbortSignal.timeout(30_000),
    body: JSON.stringify({
      schemaVersion: 1,
      installationId: "release-environment-preflight",
      clientVersion: version,
      pcVersion: version,
      channel: "stable",
      runtime: "desktop",
      target,
      arch,
      inventory: [],
    }),
  })
  if (!response.ok) throw new Error(`Fusion HTTP ${response.status}`)
  const envelope = await response.json()
  if (envelope.code !== 1)
    throw new Error("Fusion environment binding is unavailable")
  return validatePlan(envelope.data, target, arch)
}

for (const [target, arch] of platforms) {
  try {
    const components = await verifyPlatform(target, arch)
    console.log(
      `[environment-plan] ${version} ${target}/${arch}: ${components}`
    )
  } catch {
    // 下载地址包含短时票据，只输出平台和排查入口。
    console.error(
      `[environment-plan] ${version} ${target}/${arch} failed; verify Fusion binding and artifacts`
    )
    process.exitCode = 1
  }
}

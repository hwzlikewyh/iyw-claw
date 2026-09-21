#!/usr/bin/env node

import assert from "node:assert/strict"
import process from "node:process"

const version = process.argv[2]
if (!version) throw new Error("release version is required")
const adminToken =
  process.env.IYW_FUSION_ADMIN_TOKEN || process.env.FUSION_ADMIN_TOKEN
if (!adminToken) throw new Error("Fusion admin token is required")
const accessToken =
  process.env.IYW_FUSION_GATEWAY_TOKEN || process.env.FUSION_ACCESS_TOKEN
if (!accessToken) throw new Error("Fusion access token is required")

const baseUrl = (
  process.env.IYW_CLAW_FUSION_API_BASE_URL ||
  process.env.FUSION_BASE_URL ||
  "https://gateway.iyw.cn/iyw-fusion-api"
).replace(/\/$/, "")
const endpoint = new URL(
  `${baseUrl}/admin/api/agent-environment-bindings/prepare`
)
assert.equal(endpoint.protocol, "https:")
assert.ok(!endpoint.username && !endpoint.password)

const targets = [
  ["windows", "x86_64"],
  ["darwin", "x86_64"],
  ["darwin", "aarch64"],
  ["linux", "x86_64"],
  ["linux", "aarch64"],
].map(([target, arch]) => ({ target, arch }))

const response = await fetch(endpoint, {
  method: "POST",
  redirect: "error",
  headers: {
    "content-type": "application/json",
    "admin-token": adminToken,
    token: accessToken,
    "X-IYW-Admin-Actor": "github-actions-environment-binding",
  },
  signal: AbortSignal.timeout(30_000),
  body: JSON.stringify({ pcVersion: version, channel: "stable", targets }),
})
if (!response.ok) throw new Error(`Fusion HTTP ${response.status}`)
const envelope = await response.json()
if (envelope.code !== 1) {
  throw new Error(
    envelope.data?.error?.code ||
      envelope.data?.errorCode ||
      envelope.message ||
      "Fusion environment binding preparation failed"
  )
}
const result = envelope.data
assert.equal(result.pcVersion, version)
assert.equal(result.channel, "stable")
assert.ok(
  Number.isInteger(result.catalogRevision) && result.catalogRevision > 0
)
assert.ok(
  Array.isArray(result.bindings) && result.bindings.length === targets.length
)
for (const binding of result.bindings) {
  assert.ok(binding.status === "created" || binding.status === "reused")
  assert.ok(
    Number.isInteger(binding.componentCount) && binding.componentCount >= 5
  )
  console.log(
    `[environment-binding] ${version} ${binding.target}/${binding.arch}: ${binding.status} (${binding.componentCount} components)`
  )
}
console.log(`[environment-binding] catalog revision ${result.catalogRevision}`)

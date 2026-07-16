import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { test } from "node:test"
import { fileURLToPath } from "node:url"

import { createBuildPlan } from "./build-desktop.mjs"

const root = new URL("../../", import.meta.url)
const read = (path) => readFileSync(fileURLToPath(new URL(path, root)), "utf8")

test("feature-gated binaries stay outside Tauri's src/bin scan", () => {
  const cargo = read("src-tauri/Cargo.toml")

  assert.match(cargo, /path = "src\/bin_targets\/iyw_claw_server\.rs"/)
  assert.match(cargo, /path = "src\/bin_targets\/iyw_claw_mcp\.rs"/)
  assert.doesNotMatch(cargo, /path = "src\/bin\/iyw_claw_(?:server|mcp)\.rs"/)
})

test("Windows x86 runtime assets are pinned end to end", () => {
  const prepareNode = read("prepare-managed-node.ps1")
  const prepareGit = read("prepare-managed-git.ps1")
  const installNode = read("install-managed-node.ps1")
  const installGit = read("install-managed-git.ps1")
  const hooks = read("src-tauri/windows/installer-hooks.nsh")

  assert.match(prepareNode, /ValidateSet\([^)]*'x86'/)
  assert.match(prepareGit, /ValidateSet\([^)]*'x86'/)
  assert.match(prepareGit, /MinGit-\$version-32-bit\.zip/)
  assert.match(
    prepareGit,
    /04009f6150c1cec2d6779c51406c8c6a3f0133e57fa91c91eb8a030b93e68ccb/i
  )

  for (const installer of [installNode, installGit]) {
    assert.match(installer, /ValidateSet\([^)]*'x86'/)
    assert.match(installer, /\[string\]\$Architecture/)
  }

  assert.match(hooks, /!else if "\$\{ARCH\}" == "x86"/)
  assert.match(hooks, /IYW_CLAW_MANAGED_NODE_VERSION "22\.23\.1"/)
  assert.match(hooks, /node-v22\.23\.1-win-x86\.zip/)
  assert.match(hooks, /MinGit-2\.55\.0\.2-32-bit\.zip/)
  assert.match(
    hooks,
    /IYW_CLAW_PREPARE_MANAGED_NODE_SCRIPT\}"[^\r\n]+-Version "\$\{IYW_CLAW_MANAGED_NODE_VERSION\}"/
  )
  assert.match(hooks, /install-managed-node\.ps1" -Architecture "\$\{ARCH\}"/)
  assert.match(
    hooks,
    /install-managed-node[^\r\n]+-Version "\$\{IYW_CLAW_MANAGED_NODE_VERSION\}"/
  )
  assert.match(hooks, /install-managed-git\.ps1" -Architecture "\$\{ARCH\}"/)
})

test("Linux arm64 setup uses resilient apt transport", () => {
  const setup = read(".github/scripts/release/setup-linux-arm64.sh")

  assert.doesNotMatch(setup, /^deb .*http:\/\//m)
  assert.match(setup, /Acquire::ForceIPv4/)
  assert.match(setup, /Acquire::Retries/)
})

test("bundle-only builds prepare sidecars before packaging", () => {
  const plan = createBuildPlan("tauri.js", {
    bundleOnly: true,
    jobs: null,
    noSign: true,
    reuseAssets: false,
    verbose: false,
  })

  assert.deepEqual(
    plan.steps.map((step) => step.label),
    ["sidecar preparation", "NSIS bundle"]
  )
})

test("reuse-assets builds prepare sidecars before compilation", () => {
  const plan = createBuildPlan("tauri.js", {
    bundleOnly: false,
    jobs: null,
    noSign: true,
    reuseAssets: true,
    verbose: false,
  })

  assert.deepEqual(
    plan.steps.map((step) => step.label),
    ["sidecar preparation", "release build and bundle"]
  )
})

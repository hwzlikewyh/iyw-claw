import assert from "node:assert/strict"
import test from "node:test"

const { brandedInstallerName, createBuildPlan, parseBuildOptions } =
  await import("./build-desktop.mjs")

test("names the distributable installer with the public brand and version", () => {
  assert.equal(
    brandedInstallerName("iyw-claw_0.0.2_x64-setup.exe"),
    "原助手-v0.0.2-x64-setup.exe"
  )
})

test("keeps release output unchanged while limiting optional build jobs", () => {
  const options = parseBuildOptions(["--jobs=4"])
  const plan = createBuildPlan("tauri.js", options)

  assert.equal(plan.env.CARGO_BUILD_JOBS, "4")
  assert.deepEqual(plan.steps[0].args, ["tauri.js", "build", "--", "--timings"])
  assert.equal(plan.steps.length, 1)
})

test("supports bundle-only retries without rebuilding", () => {
  const options = parseBuildOptions(["--bundle-only", "--no-sign"])
  const plan = createBuildPlan("tauri.js", options)

  assert.equal(plan.env.CARGO_BUILD_JOBS, undefined)
  assert.equal(plan.steps.length, 1)
  assert.deepEqual(plan.steps[0].args, [
    "tauri.js",
    "bundle",
    "--bundles",
    "nsis",
    "--no-sign",
  ])
})

test("adds verbose diagnostics without changing Cargo release settings", () => {
  const options = parseBuildOptions(["--verbose"])
  const plan = createBuildPlan("tauri.js", options)

  assert.deepEqual(plan.steps[0].args, [
    "tauri.js",
    "build",
    "-vv",
    "--",
    "--timings",
  ])
})

test("can reuse prepared frontend and sidecar assets", () => {
  const options = parseBuildOptions(["--reuse-assets"])
  const plan = createBuildPlan("tauri.js", options)

  assert.deepEqual(plan.steps[0].args, [
    "tauri.js",
    "build",
    "--config",
    '{"build":{"beforeBuildCommand":null}}',
    "--",
    "--timings",
  ])
})

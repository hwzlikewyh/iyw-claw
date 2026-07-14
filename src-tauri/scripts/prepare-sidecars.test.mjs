import assert from "node:assert/strict"
import test from "node:test"
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"

process.env.IYW_CLAW_SKIP_SIDECAR = "1"

const { copyFileIfChanged, resolveBuildInvocation, resolveBundleCompatPaths } =
  await import("./prepare-sidecars.mjs")

test("resolves Windows bundle aliases for host and explicit target builds", () => {
  const srcTauri = join("workspace", "src-tauri")
  const triple = "x86_64-pc-windows-msvc"

  assert.deepEqual(resolveBundleCompatPaths(srcTauri, triple, ".exe"), [
    join(srcTauri, "target", "release", "iyw_claw_mcp.exe"),
    join(srcTauri, "target", triple, "release", "iyw_claw_mcp.exe"),
  ])
})

test("isolates sidecar feature artifacts from desktop release builds", () => {
  const srcTauri = join("workspace", "src-tauri")
  const triple = "x86_64-pc-windows-msvc"

  const local = resolveBuildInvocation(srcTauri, triple, ".exe")
  assert.deepEqual(local.args.slice(-2), ["--target", triple])
  assert.equal(
    local.built,
    join(srcTauri, "target", triple, "release", "iyw-claw-mcp.exe")
  )

  const cross = resolveBuildInvocation(srcTauri, triple, ".exe")
  assert.deepEqual(cross.args.slice(-2), ["--target", triple])
  assert.equal(
    cross.built,
    join(srcTauri, "target", triple, "release", "iyw-claw-mcp.exe")
  )
})

test("preserves the staged sidecar timestamp when content is unchanged", () => {
  const directory = mkdtempSync(join(tmpdir(), "iyw-claw-sidecar-"))
  const source = join(directory, "source.exe")
  const destination = join(directory, "destination.exe")
  try {
    writeFileSync(source, "same-content")
    writeFileSync(destination, "same-content")
    const originalMtime = statSync(destination).mtimeMs

    assert.equal(copyFileIfChanged(source, destination), false)
    assert.equal(statSync(destination).mtimeMs, originalMtime)

    writeFileSync(source, "new-content")
    assert.equal(copyFileIfChanged(source, destination), true)
    assert.equal(readFileSync(destination, "utf8"), "new-content")
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})

test("defers NSIS architecture selection until ARCH is defined", () => {
  const hookPath = resolve("src-tauri/windows/installer-hooks.nsh")
  const hook = readFileSync(hookPath, "utf8")
  const postInstall = hook.indexOf("!macro NSIS_HOOK_POSTINSTALL")
  const architectureBranch = hook.indexOf('!if "${ARCH}"')

  assert.ok(postInstall >= 0)
  assert.ok(architectureBranch > postInstall)
})

test("updates the NSIS output directory after changing INSTDIR", () => {
  const hookPath = resolve("src-tauri/windows/installer-hooks.nsh")
  const hook = readFileSync(hookPath, "utf8")
  const appDirectory = hook.indexOf('StrCpy $INSTDIR "$IywClawRoot\\app"')
  const outputDirectory = hook.indexOf('SetOutPath "$INSTDIR"', appDirectory)

  assert.ok(appDirectory >= 0)
  assert.ok(outputDirectory > appDirectory)
})

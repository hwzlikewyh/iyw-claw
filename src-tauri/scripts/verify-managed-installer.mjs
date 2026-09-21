import assert from "node:assert/strict"
import { spawnSync } from "node:child_process"
import { createHash, randomUUID } from "node:crypto"
import { existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs"
import { homedir, tmpdir } from "node:os"
import { dirname, join, resolve, sep } from "node:path"
import { fileURLToPath } from "node:url"
import { assertCleanInstallState, assertDisposableRunner } from "./nsis-smoke-windows.mjs"

const srcTauri = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const environment = join(homedir(), ".iyw-claw")
const installRoot = join(tmpdir(), `iyw-environment-installer-${randomUUID()}`)
const currentPath = join(environment, "inventory/environment-current.json")
const installedHelper = join(installRoot, "app/iyw-environment.exe")
const reportDir = join(srcTauri, "target/environment-installer-acceptance")
const report = { checks: [] }
const timeoutMs = 35 * 60 * 1000

function execute(file, args, env = process.env) {
  const result = spawnSync(file, args, {
    encoding: "utf8", windowsHide: true, timeout: timeoutMs,
    maxBuffer: 8 * 1024 * 1024, env,
  })
  assert.ifError(result.error)
  return result
}

function success(file, args) {
  const result = execute(file, args)
  assert.equal(result.status, 0, result.stderr)
  return result.stdout
}

function digest(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex")
}

function check(name) {
  report.checks.push(name)
  console.log(`[installer-acceptance] PASS ${name}`)
}

function files(root) {
  return readdirSync(root, { recursive: true }).map((name) => join(root, name))
}

function verifyInstall(installer, sentinels) {
  const args = ["/S", `/D=${installRoot}`]
  success(installer, args)
  assert.equal(JSON.parse(success(installedHelper, ["diagnose", "--json"])).status, "healthy")
  const before = JSON.parse(readFileSync(currentPath))
  assert.equal(before.components.length, 9)
  check("real NSIS fresh installation prepares all nine components")
  const currentDigest = digest(currentPath)
  const app = join(installRoot, "app/iyw-claw.exe")
  const appDigest = digest(app)
  const failed = execute(installer, args, {
    ...process.env, IYW_CLAW_FUSION_API_BASE_URL: "https://127.0.0.1:1",
  })
  assert.notEqual(failed.status, 0, "Offline reinstall must not report success")
  assert.match(readFileSync(join(environment, "logs/environment/last-error.log"), "utf8"), /NETWORK/)
  assert.equal(digest(currentPath), currentDigest)
  assert.equal(digest(app), appDigest)
  check("failed offline reinstall preserves the previous application and environment")
  success(installer, args)
  const after = JSON.parse(readFileSync(currentPath))
  assert.deepEqual(after.components, before.components)
  for (const path of sentinels) assert.equal(readFileSync(path, "utf8"), "preserve user data\n")
  check("online reinstall reuses component paths and preserves user data")
}

function uninstall() {
  const uninstaller = files(installRoot).find((path) => /[\\/]uninstall\.exe$/i.test(path))
  assert.ok(uninstaller, "Installed uninstaller is missing")
  success(uninstaller, ["/S", `_?=${dirname(uninstaller)}`])
  const independent = files(join(environment, "maintenance"))
    .find((path) => /[\\/]iyw-environment\.exe$/i.test(path))
  assert.ok(independent, "Independent repair helper is missing")
  assert.equal(JSON.parse(success(independent, ["diagnose", "--json"])).status, "healthy")
  check("independent helper remains usable after removing the main application")
}

function verify() {
  assertDisposableRunner()
  assert.equal(process.env.RUNNER_ENVIRONMENT, "github-hosted")
  assertCleanInstallState()
  assert.ok(!existsSync(environment), "Refusing to modify an existing environment")
  const candidates = files(resolve(process.argv[2])).filter((path) => path.endsWith("-setup.exe"))
  assert.equal(candidates.length, 1, "Exactly one Windows x64 installer is required")
  const sentinels = ["config", "plugins", "browser/profiles/default"].map((folder) => {
    const directory = join(environment, folder)
    mkdirSync(directory, { recursive: true })
    const path = join(directory, "installer-preserve.txt")
    writeFileSync(path, "preserve user data\n", { flag: "wx" })
    return path
  })
  mkdirSync(installRoot)
  verifyInstall(candidates[0], sentinels)
  uninstall()
  for (const path of sentinels) assert.equal(readFileSync(path, "utf8"), "preserve user data\n")
  assert.ok(resolve(installRoot).startsWith(resolve(tmpdir()) + sep))
  if (existsSync(installRoot)) {
    assert.ok(!lstatSync(installRoot).isSymbolicLink())
    rmSync(installRoot, { recursive: true })
  }
  check("uninstall preserves config, plugins and browser profiles")
}

try {
  verify()
  report.status = "passed"
} catch (error) {
  report.status = "failed"
  report.error = error.message.replaceAll(homedir(), "<user>")
  throw error
} finally {
  mkdirSync(reportDir, { recursive: true })
  writeFileSync(join(reportDir, "result.json"), JSON.stringify(report, null, 2))
}

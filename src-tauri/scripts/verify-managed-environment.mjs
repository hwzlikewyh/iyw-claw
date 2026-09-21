import assert from "node:assert/strict"
import { spawnSync } from "node:child_process"
import { createHash } from "node:crypto"
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs"
import { homedir } from "node:os"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const srcTauri = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const root = join(homedir(), ".iyw-claw")
const reportDir = join(srcTauri, "target/environment-acceptance")
const version = JSON.parse(readFileSync(join(srcTauri, "../package.json"))).version
const target = process.env.ENVIRONMENT_TARGET
const extension = process.platform === "win32" ? ".exe" : ""
const helper = join(srcTauri, `binaries/iyw-environment-${target}${extension}`)
const snapshotPath = join(root, "inventory/environment-current.json")
const report = { target, version, checks: [] }
const timeoutMs = 30 * 60 * 1000
const maxOutputBytes = 16 * 1024 * 1024

function run(args, expected = 0) {
  const result = spawnSync(helper, [...args, "--json"], {
    encoding: "utf8",
    windowsHide: true,
    timeout: timeoutMs,
    maxBuffer: maxOutputBytes,
  })
  assert.ifError(result.error)
  assert.equal(result.status, expected, `${args.join(" ")}: ${result.stderr}`)
  return result.stdout
}

function check(name) {
  report.checks.push(name)
  console.log(`[environment-acceptance] PASS ${name}`)
}

function digest(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex")
}

function prepare() {
  const events = run(["install", "--phase", "prepare", "--app-version", version])
  const id = readFileSync(join(root, "inventory/pending-environment.txt"), "utf8").trim()
  assert.match(id, /^[a-f0-9]{32}$/)
  return { events, id }
}

function commit(id) {
  run(["install", "--phase", "commit", "--transaction-id", id])
}

function verifyRepair(snapshot) {
  const uv = snapshot.components.find((component) => component.componentId === "uv")
  assert.ok(uv, "Acceptance version must enable uv")
  const executable = resolve(root, uv.relativePath, uv.entrypoints.uv)
  assert.ok(executable.startsWith(resolve(root, "runtime") + "/") ||
    executable.startsWith(resolve(root, "runtime") + "\\"))
  renameSync(executable, `${executable}.acceptance-damaged`)
  run(["diagnose"], 10)
  const repair = run(["repair", "--app-version", version])
  assert.doesNotMatch(repair, /"phase":"downloading"/)
  assert.equal(JSON.parse(run(["diagnose"])).status, "healthy")
  const repaired = JSON.parse(readFileSync(snapshotPath))
  for (const old of snapshot.components.filter((item) => item.componentId !== "uv")) {
    assert.deepEqual(repaired.components.find((item) => item.componentId === old.componentId), old)
  }
  check("damaged uv restored from verified cache; other components unchanged")
}

function verify() {
  assert.equal(process.env.GITHUB_ACTIONS, "true")
  assert.equal(process.env.RUNNER_ENVIRONMENT, "github-hosted", "Only disposable hosted runners are allowed")
  assert.ok(target && existsSync(helper), "Native helper must be staged")
  assert.ok(!existsSync(root), "Refusing to modify an existing user environment")
  const sentinels = ["config", "plugins", "browser/profiles/default"].map((folder) => {
    const directory = join(root, folder)
    mkdirSync(directory, { recursive: true })
    const path = join(directory, "acceptance-preserve.txt")
    writeFileSync(path, "preserve user data\n", { flag: "wx" })
    return { path, sha256: digest(path) }
  })
  const first = prepare()
  assert.match(first.events, /"phase":"downloading"/)
  commit(first.id)
  const snapshot = JSON.parse(readFileSync(snapshotPath))
  assert.equal(snapshot.pcVersion, version)
  assert.equal(snapshot.components.length, 9, "Published acceptance binding must enable all nine components")
  assert.equal(JSON.parse(run(["diagnose"])).status, "healthy")
  check("fresh nine-component install and native health probes")
  const second = prepare()
  assert.doesNotMatch(second.events, /"phase":"(?:downloading|cached)"/)
  commit(second.id)
  const beforeRetry = digest(snapshotPath)
  commit(second.id)
  assert.equal(digest(snapshotPath), beforeRetry)
  check("repeat installation downloads zero bytes; commit retry is idempotent")
  verifyRepair(snapshot)
  for (const sentinel of sentinels) assert.equal(digest(sentinel.path), sentinel.sha256)
  check("config, plugins and browser profile data preserved")
}

try {
  verify()
  report.status = "passed"
} catch (error) {
  report.status = "failed"
  report.error = String(error.message).replaceAll(root, "<environment>")
  throw error
} finally {
  mkdirSync(reportDir, { recursive: true })
  writeFileSync(join(reportDir, "result.json"), JSON.stringify(report, null, 2))
}

import { randomUUID } from "node:crypto"
import { spawnSync } from "node:child_process"
import { existsSync, readdirSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { basename, join, resolve, sep } from "node:path"
import { assertNoInstallResidue } from "./nsis-smoke-residue.mjs"

const COMMAND_MAX_BUFFER = 4 * 1024 * 1024
const COMMAND_TIMEOUT_MS = 120_000
const PRODUCT_REGISTRY_KEY = "HKCU\\Software\\iywclaw\\iyw-claw"
const REGISTRY_EXISTS_EXIT_CODE = 10
const REMOVE_MAX_RETRIES = 10
const REMOVE_RETRY_DELAY_MS = 200
const TEST_REGISTRY_KEY_PREFIX =
  "HKCU\\Software\\iywclaw\\iyw-claw-installer-test"
const TEMP_PREFIX = "iyw-claw-nsis-smoke-"
const TEST_ID_PATTERN = /^[0-9a-f]{32}$/i

function fail(message) {
  throw new Error(message)
}

function runCommand(file, args) {
  return spawnSync(file, args, {
    encoding: "utf8",
    maxBuffer: COMMAND_MAX_BUFFER,
    timeout: COMMAND_TIMEOUT_MS,
    windowsHide: true,
  })
}

export function commandDiagnostic(label, result) {
  const details = [
    `exit=${result.status ?? "none"}`,
    result.error?.message,
    result.stderr?.trim(),
    result.stdout?.trim(),
  ].filter(Boolean)
  return `${label}: ${details.join(" | ")}`
}

function registryPowerShellPath(key) {
  return `Registry::HKEY_CURRENT_USER\\${key.replace(/^HKCU\\/, "")}`
}

function registryExists(key) {
  const path = registryPowerShellPath(key)
  const script = [
    "$ErrorActionPreference = 'Stop'",
    `  $path = '${path}'`,
    `  if (Test-Path -LiteralPath $path) { exit ${REGISTRY_EXISTS_EXIT_CODE} }`,
    "  exit 0",
  ].join("; ")
  const result = runCommand("powershell.exe", [
    "-NoLogo",
    "-NoProfile",
    "-NonInteractive",
    "-Command",
    script,
  ])
  if (result.status === 0) return false
  if (result.status === REGISTRY_EXISTS_EXIT_CODE) return true
  fail(commandDiagnostic("registry query failed", result))
}

function listBlockedProcesses() {
  const result = runCommand("tasklist.exe", ["/FO", "CSV", "/NH"])
  if (result.status !== 0) fail(commandDiagnostic("tasklist failed", result))
  const names = result.stdout
    .split(/\r?\n/)
    .map((line) => /^"([^"]+)"/.exec(line)?.[1]?.toLowerCase())
    .filter(Boolean)
  return names.filter(
    (name) =>
      name === "iyw-claw.exe" ||
      name === "agent-browser.exe" ||
      (name.startsWith("iyw-claw-mcp") && name.endsWith(".exe"))
  )
}

export function assertCleanInstallState() {
  const productRegistryExists = registryExists(PRODUCT_REGISTRY_KEY)
  const processes = [...new Set(listBlockedProcesses())]
  if (!productRegistryExists && processes.length === 0) return
  const details = []
  if (productRegistryExists)
    details.push(`existing registry key ${PRODUCT_REGISTRY_KEY}`)
  if (processes.length > 0)
    details.push(`running processes ${processes.join(", ")}`)
  fail(`refusing to run installer smoke: ${details.join("; ")}`)
}

export function assertDisposableRunner() {
  if (process.platform !== "win32") fail("NSIS smoke requires a Windows runner")
  if (process.env.CI !== "true" || process.env.GITHUB_ACTIONS !== "true") {
    fail(
      "refusing to run real NSIS installation outside a disposable GitHub Actions runner"
    )
  }
}

export function createSmokeTestId() {
  return randomUUID().replaceAll("-", "")
}

export function terminateProcessTree(pid) {
  return runCommand("taskkill.exe", ["/PID", String(pid), "/T", "/F"])
}

function safeSmokeRoot(root, testId) {
  testRegistryKey(testId)
  const resolvedRoot = resolve(root)
  const resolvedTemp = `${resolve(tmpdir())}${sep}`
  return (
    resolvedRoot.startsWith(resolvedTemp) &&
    basename(resolvedRoot) === `${TEMP_PREFIX}${testId}`
  )
}

function pathInside(root, candidate) {
  const resolvedRoot = resolve(root)
    .replace(/[\\/]+$/, "")
    .toLowerCase()
  const resolvedCandidate = resolve(candidate)
    .replace(/[\\/]+$/, "")
    .toLowerCase()
  return (
    resolvedCandidate === resolvedRoot ||
    resolvedCandidate.startsWith(`${resolvedRoot}${sep}`)
  )
}

function testRegistryKey(testId) {
  if (!TEST_ID_PATTERN.test(testId))
    fail(`invalid installer test id: ${testId}`)
  return `${TEST_REGISTRY_KEY_PREFIX}\\${testId}`
}

export function installerTestArgs(testId) {
  testRegistryKey(testId)
  return [`/IYW_CLAW_TEST_MODE=${testId}`]
}

function walkFiles(root) {
  const stack = [root]
  const files = []
  while (stack.length > 0) {
    const directory = stack.pop()
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name)
      if (entry.isDirectory()) stack.push(path)
      else if (entry.isFile()) files.push(path)
    }
  }
  return files
}

function runUninstaller(options, warnings) {
  if (!existsSync(options.installRoot)) return
  const uninstaller = walkFiles(options.installRoot).find(
    (path) => basename(path).toLowerCase() === "uninstall.exe"
  )
  if (!uninstaller) {
    warnings.push("uninstaller missing from installed test package")
    return
  }
  const result = runCommand(uninstaller, [
    "/S",
    ...installerTestArgs(options.testId),
  ])
  if (result.status !== 0) {
    warnings.push(commandDiagnostic("test uninstaller failed", result))
  }
}

function escapePowerShellLiteral(value) {
  return `'${value.replaceAll("'", "''")}'`
}

function removeOwnedTestRegistryKey(options, warnings) {
  const key = testRegistryKey(options.testId)
  const path = registryPowerShellPath(key)
  const expectedRoot = resolve(options.installRoot).replace(/[\\/]+$/, "")
  const script = [
    "$ErrorActionPreference = 'Stop'",
    `  $path = ${escapePowerShellLiteral(path)}`,
    `  $expectedRoot = ${escapePowerShellLiteral(expectedRoot)}`,
    "  if (-not (Test-Path -LiteralPath $path)) { exit 0 }",
    "  $actualRoot = (Get-ItemProperty -LiteralPath $path -Name InstallRoot -ErrorAction SilentlyContinue).InstallRoot",
    "  if ($null -eq $actualRoot -or -not [string]::Equals([string]$actualRoot, $expectedRoot, [System.StringComparison]::OrdinalIgnoreCase)) { exit 10 }",
    "  try { Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction Stop } catch { if (-not (Test-Path -LiteralPath $path)) { exit 0 }; throw }",
    "  if (Test-Path -LiteralPath $path) { exit 20 }",
  ].join("; ")
  const result = runCommand("powershell.exe", [
    "-NoLogo",
    "-NoProfile",
    "-NonInteractive",
    "-Command",
    script,
  ])
  if (result.status === 0) return
  if (result.status === 10) {
    warnings.push("test registry ownership changed; skipped registry delete")
    return
  }
  warnings.push(commandDiagnostic("test registry cleanup failed", result))
}

export function cleanupInstall(options) {
  if (!safeSmokeRoot(options.smokeRoot, options.testId)) {
    fail(`refusing to clean unexpected smoke root: ${options.smokeRoot}`)
  }
  if (!pathInside(options.smokeRoot, options.installRoot)) {
    fail(`refusing to clean unexpected install root: ${options.installRoot}`)
  }
  const warnings = []
  try {
    runUninstaller(options, warnings)
  } catch (error) {
    warnings.push(`test uninstaller cleanup failed: ${error.message}`)
  }
  try {
    removeOwnedTestRegistryKey(options, warnings)
  } catch (error) {
    warnings.push(`registry cleanup failed: ${error.message}`)
  }
  rmSync(options.smokeRoot, {
    force: true,
    maxRetries: REMOVE_MAX_RETRIES,
    recursive: true,
    retryDelay: REMOVE_RETRY_DELAY_MS,
  })
  try {
    assertNoInstallResidue(options)
  } catch (error) {
    warnings.push(error.message)
  }
  return warnings
}

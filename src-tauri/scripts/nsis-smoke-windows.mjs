import { spawnSync } from "node:child_process"
import { existsSync, readdirSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { basename, join, resolve, sep } from "node:path"

const COMMAND_MAX_BUFFER = 4 * 1024 * 1024
const COMMAND_TIMEOUT_MS = 120_000
const PRODUCT_REGISTRY_KEY = "HKCU\\Software\\iywclaw\\iyw-claw"
const TEST_REGISTRY_KEY = "HKCU\\Software\\iywclaw\\iyw-claw-installer-test"
const REGISTRY_EXISTS_EXIT_CODE = 10
const TEMP_PREFIX = "iyw-claw-nsis-smoke-"
export const INSTALLER_TEST_MODE_ARG = "/IYW_CLAW_TEST_MODE=1"

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
    "try {",
    `  $path = '${path}'`,
    `  if (Test-Path -LiteralPath $path) { exit ${REGISTRY_EXISTS_EXIT_CODE} }`,
    "  exit 0",
    "} catch {",
    "  [Console]::Error.WriteLine($_.Exception.Message)",
    "  exit 20",
    "}",
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

function registryInstallRoot(key) {
  const path = registryPowerShellPath(key)
  const script = [
    "$ErrorActionPreference = 'Stop'",
    "try {",
    `  $path = '${path}'`,
    "  if (-not (Test-Path -LiteralPath $path)) { exit 0 }",
    "  $value = (Get-ItemProperty -LiteralPath $path -Name InstallRoot -ErrorAction SilentlyContinue).InstallRoot",
    "  if ($null -ne $value) { [Console]::Out.Write([string]$value) }",
    "  exit 0",
    "} catch {",
    "  [Console]::Error.WriteLine($_.Exception.Message)",
    "  exit 20",
    "}",
  ].join("; ")
  const result = runCommand("powershell.exe", [
    "-NoLogo",
    "-NoProfile",
    "-NonInteractive",
    "-Command",
    script,
  ])
  if (result.status !== 0) {
    fail(commandDiagnostic("registry root query failed", result))
  }
  const value = result.stdout.trim()
  return value || null
}

function listBlockedProcesses() {
  const result = runCommand("tasklist.exe", ["/FO", "CSV", "/NH"])
  if (result.status !== 0) {
    fail(commandDiagnostic("tasklist failed", result))
  }
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
  const testRegistryExists = registryExists(TEST_REGISTRY_KEY)
  const processes = [...new Set(listBlockedProcesses())]
  if (!productRegistryExists && !testRegistryExists && processes.length === 0)
    return
  const details = []
  if (productRegistryExists)
    details.push(`existing registry key ${PRODUCT_REGISTRY_KEY}`)
  if (testRegistryExists)
    details.push(`stale test registry key ${TEST_REGISTRY_KEY}`)
  if (processes.length > 0) {
    details.push(`running processes ${processes.join(", ")}`)
  }
  const prefix = "refusing to disturb an existing iyw-claw installation: "
  fail(`${prefix}${details.join("; ")}`)
}

export function terminateProcessTree(pid) {
  return runCommand("taskkill.exe", ["/PID", String(pid), "/T", "/F"])
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

function safeSmokeRoot(root) {
  const resolvedRoot = resolve(root)
  const resolvedTemp = `${resolve(tmpdir())}${sep}`
  return (
    resolvedRoot.startsWith(resolvedTemp) &&
    basename(resolvedRoot).startsWith(TEMP_PREFIX)
  )
}

function samePath(left, right) {
  const normalize = (value) =>
    resolve(value)
      .replace(/[\\/]+$/, "")
      .toLowerCase()
  return normalize(left) === normalize(right)
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

function registryOwnedBy(root) {
  const currentRoot = registryInstallRoot(TEST_REGISTRY_KEY)
  return currentRoot !== null && samePath(currentRoot, root)
}

function runUninstaller(installRoot, warnings) {
  if (!existsSync(installRoot)) return
  if (!registryOwnedBy(installRoot)) {
    warnings.push("registry ownership changed; skipped uninstaller")
    return
  }
  const processes = [...new Set(listBlockedProcesses())]
  if (processes.length > 0) {
    warnings.push(
      `running processes appeared; skipped uninstaller: ${processes.join(", ")}`
    )
    return
  }
  const uninstaller = walkFiles(installRoot).find(
    (path) => basename(path).toLowerCase() === "uninstall.exe"
  )
  if (!uninstaller) {
    warnings.push("uninstaller missing from installed product")
    return
  }
  const result = runCommand(uninstaller, ["/S", INSTALLER_TEST_MODE_ARG])
  if (result.status !== 0) {
    warnings.push(commandDiagnostic("uninstaller failed", result))
  }
}

function removeTestRegistryKey(expectedRoot, warnings) {
  if (!registryExists(TEST_REGISTRY_KEY)) return
  if (!registryOwnedBy(expectedRoot)) {
    warnings.push("registry ownership changed; skipped registry delete")
    return
  }
  const result = runCommand("reg.exe", ["delete", TEST_REGISTRY_KEY, "/f"])
  if (result.status !== 0) {
    warnings.push(commandDiagnostic("registry delete failed", result))
  }
  if (registryExists(TEST_REGISTRY_KEY)) {
    const remainingRoot = registryInstallRoot(TEST_REGISTRY_KEY)
    if (!remainingRoot || !samePath(remainingRoot, expectedRoot)) {
      warnings.push("registry key changed or remains after cleanup")
    } else {
      warnings.push("product registry key remains")
    }
  }
}

export function cleanupInstall(options) {
  if (!safeSmokeRoot(options.smokeRoot)) {
    fail(`refusing to clean unexpected smoke root: ${options.smokeRoot}`)
  }
  if (!pathInside(options.smokeRoot, options.installRoot)) {
    fail(`refusing to clean unexpected install root: ${options.installRoot}`)
  }
  const warnings = []
  const registryRoot = registryInstallRoot(TEST_REGISTRY_KEY)
  const ownsRegistry =
    registryRoot !== null && samePath(registryRoot, options.installRoot)
  if (registryRoot && !ownsRegistry) {
    warnings.push(
      `registry ownership mismatch; expected ${options.installRoot}, found ${registryRoot}`
    )
  }
  try {
    if (ownsRegistry) runUninstaller(options.installRoot, warnings)
    else if (!registryRoot)
      warnings.push("product registry key missing; skipped uninstaller")
  } catch (error) {
    warnings.push(`uninstaller cleanup failed: ${error.message}`)
  }
  try {
    if (ownsRegistry) removeTestRegistryKey(options.installRoot, warnings)
  } catch (error) {
    warnings.push(`registry cleanup failed: ${error.message}`)
  }
  rmSync(options.smokeRoot, { force: true, recursive: true })
  return warnings
}

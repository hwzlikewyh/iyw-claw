import { spawnSync } from "node:child_process"
import { existsSync, readdirSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { basename, join, resolve, sep } from "node:path"

const COMMAND_MAX_BUFFER = 4 * 1024 * 1024
const COMMAND_TIMEOUT_MS = 120_000
const PRODUCT_REGISTRY_KEY = "HKCU\\Software\\iywclaw\\iyw-claw"
const REGISTRY_EXISTS_EXIT_CODE = 10
const TEMP_PREFIX = "iyw-claw-nsis-smoke-"

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

function productRegistryExists() {
  const script = [
    "$ErrorActionPreference = 'Stop'",
    "try {",
    "  $path = 'Registry::HKEY_CURRENT_USER\\Software\\iywclaw\\iyw-claw'",
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
  const registryExists = productRegistryExists()
  const processes = [...new Set(listBlockedProcesses())]
  if (!registryExists && processes.length === 0) return
  const details = []
  if (registryExists)
    details.push(`existing registry key ${PRODUCT_REGISTRY_KEY}`)
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

function runUninstaller(installRoot, warnings) {
  if (!existsSync(installRoot)) return
  const uninstaller = walkFiles(installRoot).find(
    (path) => basename(path).toLowerCase() === "uninstall.exe"
  )
  if (!uninstaller) {
    warnings.push("uninstaller missing from installed product")
    return
  }
  const result = runCommand(uninstaller, ["/S"])
  if (result.status !== 0) {
    warnings.push(commandDiagnostic("uninstaller failed", result))
  }
}

function removeProductRegistryKey(warnings) {
  if (!productRegistryExists()) return
  const result = runCommand("reg.exe", ["delete", PRODUCT_REGISTRY_KEY, "/f"])
  if (result.status !== 0) {
    warnings.push(commandDiagnostic("registry delete failed", result))
  }
  if (productRegistryExists()) warnings.push("product registry key remains")
}

export function cleanupInstall(options) {
  if (!safeSmokeRoot(options.smokeRoot)) {
    fail(`refusing to clean unexpected smoke root: ${options.smokeRoot}`)
  }
  const warnings = []
  try {
    runUninstaller(options.installRoot, warnings)
  } catch (error) {
    warnings.push(`uninstaller cleanup failed: ${error.message}`)
  }
  try {
    removeProductRegistryKey(warnings)
  } catch (error) {
    warnings.push(`registry cleanup failed: ${error.message}`)
  }
  rmSync(options.smokeRoot, { force: true, recursive: true })
  return warnings
}

import { spawnSync } from "node:child_process"
import { join, resolve } from "node:path"

const COMMAND_MAX_BUFFER = 4 * 1024 * 1024
const COMMAND_TIMEOUT_MS = 120_000
const PRODUCT_REGISTRY_KEY = "Software\\iywclaw\\iyw-claw"
const TEST_REGISTRY_KEY_PREFIX = "Software\\iywclaw\\iyw-claw-installer-test"
const UNINSTALL_REGISTRY_KEY =
  "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"

function runPowerShell(lines) {
  const result = spawnSync(
    "powershell.exe",
    ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", lines.join("; ")],
    {
      encoding: "utf8",
      maxBuffer: COMMAND_MAX_BUFFER,
      timeout: COMMAND_TIMEOUT_MS,
      windowsHide: true,
    }
  )
  if (result.status !== 0) {
    const detail = [result.error?.message, result.stderr, result.stdout]
      .filter(Boolean)
      .join(" | ")
    throw new Error(`NSIS residue query failed: ${detail || result.status}`)
  }
  return result.stdout
    .split(/\r?\n/)
    .map((value) => value.trim())
    .filter(Boolean)
}

function literal(value) {
  return `'${String(value).replaceAll("'", "''")}'`
}

function registryResidue(options) {
  const root = resolve(options.smokeRoot).replace(/[\\/]+$/, "")
  const testKey = `${TEST_REGISTRY_KEY_PREFIX}\\${options.testId}`
  return runPowerShell([
    "$ErrorActionPreference = 'Stop'",
    `$root = ${literal(root)}`,
    `$paths = @(${literal(`Registry::HKEY_CURRENT_USER\\${testKey}`)}, ${literal(`Registry::HKEY_CURRENT_USER\\${PRODUCT_REGISTRY_KEY}`)})`,
    `$uninstallRoot = ${literal(`Registry::HKEY_CURRENT_USER\\${UNINSTALL_REGISTRY_KEY}`)}`,
    "$residue = @($paths | Where-Object { Test-Path -LiteralPath $_ })",
    "if (Test-Path -LiteralPath $uninstallRoot) {",
    "  Get-ChildItem -LiteralPath $uninstallRoot | ForEach-Object {",
    "    $values = Get-ItemProperty -LiteralPath $_.PSPath -ErrorAction SilentlyContinue | Out-String",
    '    if ($values -like "*$root*") { $residue += $_.PSPath }',
    "  }",
    "}",
    "$residue | ForEach-Object { Write-Output $_ }",
  ])
}

function shortcutResidue(options) {
  const root = resolve(options.smokeRoot)
    .replace(/[\\/]+$/, "")
    .toLowerCase()
  const locations = [
    process.env.USERPROFILE && join(process.env.USERPROFILE, "Desktop"),
    process.env.APPDATA &&
      join(process.env.APPDATA, "Microsoft", "Windows", "Start Menu"),
  ].filter(Boolean)
  return runPowerShell([
    "$ErrorActionPreference = 'Stop'",
    `$root = ${literal(root)}`,
    `$locations = @(${locations.map(literal).join(", ")})`,
    "$shell = New-Object -ComObject WScript.Shell",
    "$residue = @()",
    "foreach ($location in $locations) {",
    "  if (-not (Test-Path -LiteralPath $location)) { continue }",
    "  Get-ChildItem -LiteralPath $location -Filter '*.lnk' -File -Recurse | ForEach-Object {",
    "    $target = $shell.CreateShortcut($_.FullName).TargetPath.ToLowerInvariant()",
    "    if ($target.StartsWith($root)) { $residue += $_.FullName }",
    "  }",
    "}",
    "$residue | ForEach-Object { Write-Output $_ }",
  ])
}

export function assertNoInstallResidue(options) {
  const residue = [...registryResidue(options), ...shortcutResidue(options)]
  if (residue.length > 0)
    throw new Error(`NSIS smoke left install residue: ${residue.join(", ")}`)
}

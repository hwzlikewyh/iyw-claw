#!/usr/bin/env node

import { spawnSync } from "node:child_process"
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  unlinkSync,
  writeFileSync,
} from "node:fs"
import { tmpdir } from "node:os"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const COMMAND_MAX_BUFFER = 4 * 1024 * 1024
const COMMAND_TIMEOUT_MS = 30_000
const REMOVE_MAX_RETRIES = 10
const REMOVE_RETRY_DELAY_MS = 200

function fail(message) {
  throw new Error(message)
}

function commandDiagnostic(label, result) {
  const details = [
    `exit=${result.status ?? "none"}`,
    result.error?.message,
    result.stderr?.trim(),
    result.stdout?.trim(),
  ].filter(Boolean)
  return `${label}: ${details.join(" | ")}`
}

function runCommand(file, args, cwd) {
  return spawnSync(file, args, {
    cwd,
    encoding: "utf8",
    maxBuffer: COMMAND_MAX_BUFFER,
    timeout: COMMAND_TIMEOUT_MS,
    windowsHide: true,
  })
}

function resolveMakensis() {
  const localAppData = process.env.LOCALAPPDATA
  if (!localAppData) fail("LOCALAPPDATA is unavailable")
  const root = join(localAppData, "tauri", "NSIS")
  const candidates = [
    join(root, "makensis.exe"),
    join(root, "Bin", "makensis.exe"),
  ]
  const makensis = candidates.find((candidate) => existsSync(candidate))
  if (!makensis) fail(`makensis not found below ${root}`)
  return makensis
}

function powerShellLiteral(value) {
  return `'${value.replaceAll("'", "''")}'`
}

function probeSource(sourcePath, executablePath, resultPath) {
  return [
    '!include "FileFunc.nsh"',
    '!define IYW_CLAW_INSTALL_REGISTRY_KEY "Software\\iywclaw\\iyw-claw"',
    "Var IywClawInstallRegistryKey",
    "Var IywClawRoot",
    `!include "${sourcePath}"`,
    'Name "iyw-claw installer test-id probe"',
    `OutFile "${executablePath}"`,
    "RequestExecutionLevel user",
    "SilentInstall silent",
    "Function .onInit",
    "  Call IywClawConfigureInstallerMode",
    `  FileOpen $0 "${resultPath}" w`,
    '  FileWrite $0 "$IywClawInstallerTestMode|$IywClawInstallerTestId"',
    "  FileClose $0",
    "  Quit",
    "FunctionEnd",
    "Section",
    "SectionEnd",
    "",
  ].join("\n")
}

function waitForProbe(executable, testId, cwd) {
  const script = [
    "$ErrorActionPreference = 'Stop'",
    `$process = Start-Process -FilePath ${powerShellLiteral(executable)} ` +
      `-ArgumentList @('/S', ${powerShellLiteral(`/IYW_CLAW_TEST_MODE=${testId}`)}) ` +
      "-WindowStyle Hidden -Wait -PassThru",
    "if ($null -eq $process.ExitCode) { exit 20 }",
  ].join("; ")
  const result = runCommand(
    "powershell.exe",
    ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script],
    cwd
  )
  if (result.status !== 0)
    fail(commandDiagnostic("test mode probe failed", result))
}

function verifyCase(options) {
  const { cwd, entry, executable, resultPath } = options
  if (existsSync(resultPath)) unlinkSync(resultPath)
  waitForProbe(executable, entry.testId, cwd)
  if (!existsSync(resultPath)) fail(`${entry.name}: probe result missing`)
  const actual = readFileSync(resultPath, "utf8")
  if (actual !== entry.expected) {
    fail(`${entry.name}: expected ${entry.expected}, received ${actual}`)
  }
  console.log(`${entry.name}: ${actual}`)
}

function main() {
  if (process.platform !== "win32")
    fail("NSIS test mode probe requires Windows")
  const scriptDir = dirname(fileURLToPath(import.meta.url))
  const sourcePath = resolve(scriptDir, "../windows/installer-test-mode.nsh")
  const probeRoot = mkdtempSync(join(tmpdir(), "iyw-claw-nsis-mode-"))
  const probeScript = join(probeRoot, "probe.nsi")
  const executable = join(probeRoot, "probe.exe")
  const resultPath = join(probeRoot, "result.txt")
  const lower = "0123456789abcdef0123456789abcdef"
  const upper = lower.toUpperCase()
  const cases = [
    { name: "lower", testId: lower, expected: `1|${lower}` },
    { name: "upper", testId: upper, expected: `1|${upper}` },
    { name: "length-31", testId: lower.slice(0, 31), expected: "invalid|" },
    { name: "length-33", testId: `${lower}0`, expected: "invalid|" },
    { name: "non-hex", testId: `g${lower.slice(1)}`, expected: "invalid|" },
  ]
  try {
    writeFileSync(probeScript, probeSource(sourcePath, executable, resultPath))
    const compiled = runCommand(
      resolveMakensis(),
      ["/NOCD", "/V2", probeScript],
      probeRoot
    )
    if (compiled.status !== 0)
      fail(commandDiagnostic("makensis failed", compiled))
    for (const entry of cases) {
      verifyCase({ cwd: probeRoot, entry, executable, resultPath })
    }
  } finally {
    rmSync(probeRoot, {
      force: true,
      maxRetries: REMOVE_MAX_RETRIES,
      recursive: true,
      retryDelay: REMOVE_RETRY_DELAY_MS,
    })
  }
}

main()

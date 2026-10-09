import { execFileSync } from "node:child_process"
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync } from "node:fs"
import { join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { windowsImports } from "./xinghe-worker-binary.mjs"
import { verifyWin7FunctionImports } from "./windows7-imports.mjs"

const ROOT = fileURLToPath(new URL("../..", import.meta.url))
export const WIN7_TARGET = "x86_64-win7-windows-msvc"
export const WIN7_X86_TARGET = "i686-win7-windows-msvc"
export const WIN7_TARGETS = [WIN7_TARGET, WIN7_X86_TARGET]
export const WIN7_TOOLCHAIN = "nightly-2026-04-15"
const PE_HEADER_POINTER = 0x3c
const PE_MACHINE_OFFSET = 4
const PE_MACHINES = {
  [WIN7_TARGET]: 0x8664,
  [WIN7_X86_TARGET]: 0x014c,
}
const PE_SIGNATURE = 0x4550
const WIN7_UNAVAILABLE_DLLS = new Set([
  "bcryptprimitives.dll",
  "combase.dll",
  "api-ms-win-core-apiquery-l2-1-0.dll",
  "api-ms-win-core-winrt-l1-1-0.dll",
  "api-ms-win-core-winrt-string-l1-1-0.dll",
  "api-ms-win-core-winrt-error-l1-1-0.dll",
  "api-ms-win-security-base-l1-2-2.dll",
])

export function verifyWin7Imports(bytes, target) {
  if (!WIN7_TARGETS.includes(target)) return
  const unsupported = windowsImports(bytes, target).filter((name) =>
    WIN7_UNAVAILABLE_DLLS.has(name)
  )
  if (unsupported.length)
    throw new Error(
      `Win7 binary imports unavailable system libraries: ${unsupported.join(", ")}`
    )
  verifyWin7FunctionImports(bytes)
}

export function isWin7Target(target) {
  return WIN7_TARGETS.includes(target)
}

export function win7Architecture(target) {
  if (target === WIN7_TARGET) return "x64"
  if (target === WIN7_X86_TARGET) return "x86"
  throw new Error(`Unsupported Win7 target: ${target}`)
}

export function resolveWindowsTarget() {
  const target =
    process.env.TAURI_TARGET_TRIPLE ||
    process.env.CARGO_BUILD_TARGET ||
    execFileSync("rustc", ["-vV"], { encoding: "utf8" })
      .split(/\r?\n/)
      .find((line) => line.startsWith("host:"))
      ?.slice(5)
      .trim()
  if (!target?.endsWith("-windows-msvc"))
    throw new Error(`Unsupported Windows desktop target: ${target}`)
  return target
}

export function windowsBuildEnvironment(target, environment = process.env) {
  const env = { ...environment, TAURI_TARGET_TRIPLE: target }
  if (isWin7Target(target)) {
    env.RUSTUP_TOOLCHAIN ||= WIN7_TOOLCHAIN
    env.CARGO_UNSTABLE_BUILD_STD = "std,panic_abort"
  }
  return env
}

function verifyRuntimeArchitecture(executable, target) {
  const bytes = readFileSync(executable)
  if (
    bytes.length <= PE_HEADER_POINTER + 4 ||
    bytes.toString("ascii", 0, 2) !== "MZ"
  )
    throw new Error("WebView2 Runtime must contain a valid Windows executable")
  const pe = bytes.readUInt32LE(PE_HEADER_POINTER)
  if (
    pe + PE_MACHINE_OFFSET + 2 > bytes.length ||
    !PE_MACHINES[target] ||
    bytes.readUInt32LE(pe) !== PE_SIGNATURE ||
    bytes.readUInt16LE(pe + PE_MACHINE_OFFSET) !== PE_MACHINES[target]
  )
    throw new Error(
      `Win7 WebView2 Runtime must use ${win7Architecture(target)} binaries`
    )
}

function verifyRuntimeVersion(executable) {
  // pwsh 的模块目录不能传给 Windows PowerShell 5.1，否则安全模块无法加载。
  const env = { ...process.env, IYW_WIN7_RUNTIME_EXECUTABLE: executable }
  delete env.PSModulePath
  const version = execFileSync(
    "powershell.exe",
    [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      "$ErrorActionPreference = 'Stop'; $file = Get-Item -LiteralPath $env:IYW_WIN7_RUNTIME_EXECUTABLE; " +
        "foreach ($name in @('msedgewebview2.exe', 'msedge.dll', 'notification_helper.exe')) { " +
        "$signature = Get-AuthenticodeSignature -LiteralPath (Join-Path $file.DirectoryName $name); " +
        "if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') { throw 'WebView2 requires valid Microsoft signatures' } }; " +
        "$file.VersionInfo.ProductVersion",
    ],
    {
      encoding: "utf8",
      windowsHide: true,
      env,
    }
  ).trim()
  if (!/^109\.\d+\.\d+\.\d+$/.test(version))
    throw new Error(`Win7 requires WebView2 Runtime 109; found ${version}`)
  return version
}

function stageRuntime(directory, version, target) {
  const architecture = win7Architecture(target)
  const relative =
    architecture === "x64"
      ? `resources/webview2-win7/${version}`
      : `resources/webview2-win7/${architecture}/${version}`
  const destination = join(
    process.env.IYW_CLAW_BUILD_ROOT || ROOT,
    "src-tauri",
    relative
  )
  if (resolve(directory) !== resolve(destination) && !existsSync(destination)) {
    mkdirSync(join(destination, ".."), { recursive: true })
    cpSync(resolve(directory), destination, { recursive: true })
  }
  const executable = join(destination, "msedgewebview2.exe")
  verifyRuntimeArchitecture(executable, target)
  if (verifyRuntimeVersion(executable) !== version)
    throw new Error("Staged Win7 WebView2 Runtime version mismatch")
  return relative
}

export function win7WebviewConfig(target) {
  if (!isWin7Target(target)) return null
  const architecture = win7Architecture(target)
  const stagedRoot = join(
    process.env.IYW_CLAW_BUILD_ROOT || ROOT,
    "src-tauri",
    "resources",
    "webview2-win7"
  )
  const stagedDirectory =
    architecture === "x64" ? stagedRoot : join(stagedRoot, architecture)
  const stagedCandidates = existsSync(stagedDirectory)
    ? readdirSync(stagedDirectory, { withFileTypes: true })
        .filter((entry) => entry.isDirectory() && /^109\./.test(entry.name))
        .map((entry) => join(stagedDirectory, entry.name))
    : []
  const configured = process.env.IYW_WIN7_WEBVIEW2_FIXED_RUNTIME_PATH
  if (!configured && stagedCandidates.length > 1)
    throw new Error("Select one Win7 WebView2 Runtime with IYW_WIN7_WEBVIEW2_FIXED_RUNTIME_PATH")
  const directory = configured || stagedCandidates[0]
  const executable = directory && join(resolve(directory), "msedgewebview2.exe")
  if (!executable || !existsSync(executable))
    throw new Error(
      `IYW_WIN7_WEBVIEW2_FIXED_RUNTIME_PATH must contain extracted WebView2 Runtime 109 ${win7Architecture(target)}`
    )
  for (const name of ["msedge.dll", "icudtl.dat"]) {
    if (!existsSync(join(resolve(directory), name)))
      throw new Error(`WebView2 fixed runtime is incomplete: missing ${name}`)
  }
  verifyRuntimeArchitecture(executable, target)
  verifyRuntimeArchitecture(join(resolve(directory), "msedge.dll"), target)
  const version = verifyRuntimeVersion(executable)
  // 配置必须使用应用内相对路径，绝不能把构建机绝对目录写入运行时上下文。
  const runtimePath = stageRuntime(directory, version, target)
  return {
    bundle: {
      externalBin: ["binaries/iyw-environment"],
      windows: {
        minimumWebview2Version: null,
        webviewInstallMode: { type: "fixedRuntime", path: runtimePath },
        nsis: {
          installerHooks:
            target === WIN7_X86_TARGET
              ? "./windows/installer-win7-x86-hooks.nsh"
              : "./windows/installer-win7-hooks.nsh",
        },
      },
    },
  }
}

function scriptStep(script, target) {
  return {
    label: script,
    args: [
      join(ROOT, "src-tauri/scripts", `${script}.mjs`),
      "--target",
      target,
    ],
  }
}

function windowsBuildArgs(tauriCli, options, target, signingConfigPath) {
  const args = [
    tauriCli,
    options.bundleOnly ? "bundle" : "build",
    "--target",
    target,
  ]
  if (options.bundleOnly) args.push("--bundles", "nsis")
  else
    args.push(
      "--config",
      JSON.stringify({
        build: {
          beforeBuildCommand: options.reuseAssets ? null : "pnpm build",
        },
      })
    )
  if (signingConfigPath) args.push("--config", signingConfigPath)
  const webview = win7WebviewConfig(target)
  if (webview) args.push("--config", JSON.stringify(webview))
  if (options.verbose) args.push("-vv")
  if (options.noSign) args.push("--no-sign")
  if (!options.bundleOnly) args.push("--", "--timings")
  return args
}

export function createWindowsBuildPlan(tauriCli, options, signingConfigPath) {
  const target = resolveWindowsTarget()
  const env = windowsBuildEnvironment(target)
  if (options.jobs) env.CARGO_BUILD_JOBS = String(options.jobs)
  return {
    env,
    steps: [
      scriptStep("prepare-sidecars", target),
      scriptStep("prepare-xinghe-worker", target),
      {
        label: options.bundleOnly ? "NSIS bundle" : "release build and bundle",
        args: windowsBuildArgs(tauriCli, options, target, signingConfigPath),
      },
      scriptStep("verify-xinghe-worker-bundle", target),
    ],
  }
}

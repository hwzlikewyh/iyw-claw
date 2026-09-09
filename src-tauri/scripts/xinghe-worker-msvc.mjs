import { execFileSync } from "node:child_process"
import { existsSync, readFileSync, readdirSync } from "node:fs"
import { join } from "node:path"
import { helperNames, windowsRuntimeImports } from "./xinghe-worker-binary.mjs"

const ARCHITECTURES = {
  "x86_64-pc-windows-msvc": "x64",
  "i686-pc-windows-msvc": "x86",
  "aarch64-pc-windows-msvc": "arm64",
}
// 与沙箱辅助程序的运行库复制白名单保持一致。
const RUNTIME_NAMES = ["vcruntime140.dll", "vcruntime140_1.dll"]

function redistRoots(environment) {
  if (environment.VCToolsRedistDir) return [environment.VCToolsRedistDir]
  const vswhere = join(
    environment["ProgramFiles(x86)"] || "C:\\Program Files (x86)",
    "Microsoft Visual Studio",
    "Installer",
    "vswhere.exe"
  )
  const installation =
    environment.VSINSTALLDIR ||
    execFileSync(
      vswhere,
      [
        "-latest",
        "-products",
        "*",
        "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
        "-property",
        "installationPath",
      ],
      { encoding: "utf8", windowsHide: true }
    ).trim()
  if (!installation)
    throw new Error("Visual Studio C++ redistributable is unavailable")
  const root = join(installation, "VC", "Redist", "MSVC")
  return readdirSync(root, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && /^\d+\.\d+/.test(entry.name))
    .sort((left, right) =>
      right.name.localeCompare(left.name, "en", { numeric: true })
    )
    .map((entry) => join(root, entry.name))
}

function findRuntimeDirectory(target, environment) {
  const architecture = ARCHITECTURES[target]
  if (!architecture) throw new Error(`unsupported MSVC target: ${target}`)
  for (const root of redistRoots(environment)) {
    const directory = join(root, architecture)
    if (!existsSync(directory)) continue
    const crt = readdirSync(directory).find((name) =>
      /^Microsoft\.VC\d+\.CRT$/.test(name)
    )
    if (crt) return join(directory, crt)
  }
  throw new Error(
    `Visual Studio release CRT is unavailable for ${architecture}`
  )
}

function requiredRuntimeNames(resourceRoot, target) {
  const binaries = ["iyw_xinghe_worker.dll", ...helperNames(target)]
  return new Set(
    binaries.flatMap((name) =>
      windowsRuntimeImports(readFileSync(join(resourceRoot, name)), target)
    )
  )
}

export function verifyWindowsRuntime(resourceRoot, target) {
  if (!target.includes("windows")) return []
  const required = requiredRuntimeNames(resourceRoot, target)
  for (const name of required) {
    if (!RUNTIME_NAMES.includes(name))
      throw new Error(
        `unsupported MSVC dependency for sandbox helpers: ${name}`
      )
    const path = join(resourceRoot, name)
    if (!existsSync(path))
      throw new Error(`bundled MSVC runtime is missing: ${name}`)
    const value = readFileSync(path)
    const dependencies = windowsRuntimeImports(value, target)
    const pe = value.readUInt32LE(0x3c)
    if ((value.readUInt16LE(pe + 22) & 0x2000) === 0)
      throw new Error(`bundled MSVC runtime is not a DLL: ${name}`)
    for (const dependency of dependencies) required.add(dependency)
  }
  return [...required]
}

export function stageWindowsRuntime(
  { target, resourceRoot, stageLibrary },
  environment = process.env
) {
  if (!target.includes("windows")) return
  const required = requiredRuntimeNames(resourceRoot, target)
  if (required.size === 0) return
  const directory = findRuntimeDirectory(target, environment)
  for (const name of required) {
    if (!RUNTIME_NAMES.includes(name))
      throw new Error(
        `unsupported MSVC dependency for sandbox helpers: ${name}`
      )
    const source = join(directory, name)
    for (const dependency of windowsRuntimeImports(
      readFileSync(source),
      target
    ))
      required.add(dependency)
    stageLibrary(source, name)
    console.log(`[xinghe-worker] bundled Microsoft runtime: ${name}`)
  }
  verifyWindowsRuntime(resourceRoot, target)
}

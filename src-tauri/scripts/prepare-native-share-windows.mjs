import { execFileSync } from "node:child_process"
import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  rmSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs"
import { dirname, join, resolve } from "node:path"
import {
  discoverSigntool,
  resolveSignMode,
  signFiles,
} from "./sign-windows.mjs"

const DEVELOPMENT_PUBLISHER = "CN=IywAssistant Development"

function run(tool, args, options = {}) {
  return execFileSync(tool, args, {
    windowsHide: true,
    stdio: "inherit",
    ...options,
  })
}

function publisher(root, signed) {
  if (!signed) return DEVELOPMENT_PUBLISHER
  if (process.env.IYW_SHARE_PUBLISHER) return process.env.IYW_SHARE_PUBLISHER
  return run(
    "powershell.exe",
    [
      "-NoProfile",
      "-NonInteractive",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      join(root, "windows/certificate-subject.ps1"),
    ],
    { encoding: "utf8", stdio: "pipe" }
  ).trim()
}

function xml(text) {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
}

function sdkTool(name) {
  const tool = join(dirname(discoverSigntool()), name)
  if (!existsSync(tool)) throw new Error(`Windows SDK tool missing: ${name}`)
  return tool
}

function preparePackageFiles({ root, output, stage, subject, version }) {
  if (resolve(stage) !== resolve(root, "build/msix"))
    throw new Error("Unexpected MSIX staging directory")
  rmSync(stage, { recursive: true, force: true })
  mkdirSync(stage, { recursive: true })
  run("powershell.exe", [
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    join(root, "windows/prepare-assets.ps1"),
    "-Source",
    resolve(root, "../icons/128x128.png"),
    "-Output",
    join(output, "Assets"),
  ])
  const manifest = readFileSync(join(root, "windows/AppxManifest.xml"), "utf8")
    .replaceAll("{{publisher}}", xml(subject))
    .replaceAll("{{version}}", version)
  writeFileSync(join(stage, "AppxManifest.xml"), `\uFEFF${manifest}`)
  mkdirSync(join(stage, "Assets"), { recursive: true })
  cpSync(join(output, "Assets"), join(stage, "Assets"), { recursive: true })
}

function createResourceIndex({ root, stage, output }) {
  const config = join(root, "build/priconfig.xml")
  run(sdkTool("makepri.exe"), [
    "createconfig",
    "/cf",
    config,
    "/dq",
    "lang-zh-CN",
    "/o",
  ])
  run(sdkTool("makepri.exe"), [
    "new",
    "/pr",
    stage,
    "/cf",
    config,
    "/of",
    join(stage, "resources.pri"),
    "/in",
    "IywAssistant.Desktop",
    "/o",
  ])
  for (const name of readdirSync(stage).filter((name) => name.endsWith(".pri")))
    copyFileSync(join(stage, name), join(output, name))
}

export function packWindowsShare(context) {
  preparePackageFiles(context)
  createResourceIndex(context)
  const { output, stage } = context
  const msix = join(output, "IywAssistant.ShareTarget.msix")
  run(sdkTool("makeappx.exe"), ["pack", "/d", stage, "/p", msix, "/nv", "/o"])
  return msix
}

export function prepareWindowsShare(target, root) {
  if (process.platform !== "win32")
    throw new Error("Build the share target on Windows")
  const signed =
    process.env.IYW_SHARE_UNSIGNED !== "1" && resolveSignMode() !== "none"
  const subject = publisher(root, signed)
  const version = `${JSON.parse(readFileSync(join(root, "../../package.json"), "utf8")).version}.0`
  const output = join(root, "resources")
  const stage = join(root, "build/msix")
  const context = { target, root, subject, output, stage, version }
  writeFileSync(
    join(output, "config.json"),
    JSON.stringify({
      schemaVersion: 1,
      signed,
      packageVersion: version,
      receiver: "main",
      publisher: subject,
    })
  )
  if (!signed) {
    console.log(
      "[native-share] unsigned build: manual ZIP import remains available"
    )
    return
  }
  const packageFile = packWindowsShare(context)
  if ((process.env.IYW_CLAW_SAFENET_PIN ?? "").trim())
    run(process.execPath, [
      join(root, "../scripts/sign-staged-windows.mjs"),
      packageFile,
    ])
  else signFiles([packageFile])
  console.log(
    `[native-share] signed identity package for the main application prepared for ${target}`
  )
}

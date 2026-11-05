import { execFileSync } from "node:child_process"
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  rmSync,
  readFileSync,
  writeFileSync,
} from "node:fs"
import { dirname, join, resolve } from "node:path"
import { discoverSigntool, resolveSignMode } from "./sign-windows.mjs"
import { verifyWindowsShare } from "./verify-native-share.mjs"

const DEVELOPMENT_PUBLISHER = "CN=IywAssistant Development"

function signShareFile(root, file) {
  // 复用正式发布的硬件签名入口，避免绕过 token/PIN 与签名后校验。
  run(process.execPath, [
    join(root, "../scripts/sign-staged-windows.mjs"),
    file,
  ])
}

function run(tool, args, options = {}) {
  const environment = options.env || process.env
  return execFileSync(tool, args, {
    windowsHide: true,
    stdio: "inherit",
    ...options,
    env: tool.toLowerCase().endsWith("powershell.exe")
      ? Object.fromEntries(
          Object.entries(environment).filter(
            ([name]) => name.toUpperCase() !== "PSMODULEPATH"
          )
        )
      : environment,
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

function buildReceiver({ target, root, subject, output }) {
  run(
    "cargo",
    [
      "build",
      "--release",
      "--locked",
      "--manifest-path",
      join(root, "../share-target/Cargo.toml"),
      "--target",
      target,
      "--target-dir",
      join(root, "../share-target/target"),
    ],
    { env: { ...process.env, IYW_SHARE_PUBLISHER: subject } }
  )
  const executable = join(output, "iyw-share-target.exe")
  copyFileSync(
    join(root, `../share-target/target/${target}/release/iyw-share-target.exe`),
    executable
  )
  copyFileSync(join(root, "windows/register.ps1"), join(output, "register.ps1"))
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
  for (const size of [44, 150])
    copyFileSync(
      join(output, `Assets/Square${size}x${size}Logo.png`),
      join(stage, `Assets/Square${size}x${size}Logo.png`)
    )
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
    "IywAssistant.ShareTarget",
    "/o",
  ])
  copyFileSync(join(stage, "resources.pri"), join(output, "resources.pri"))
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
  buildReceiver(context)
  writeFileSync(
    join(output, "config.json"),
    JSON.stringify({ schemaVersion: 1, signed, packageVersion: version })
  )
  if (!signed) {
    console.log(
      "[native-share] unsigned build: manual ZIP import remains available"
    )
    return
  }
  signShareFile(root, join(output, "iyw-share-target.exe"))
  signShareFile(root, packWindowsShare(context))
  verifyWindowsShare({
    root: output,
    version: version.replace(/\.0$/, ""),
    requireSigned: true,
  })
  console.log(`[native-share] signed Share Target prepared for ${target}`)
}

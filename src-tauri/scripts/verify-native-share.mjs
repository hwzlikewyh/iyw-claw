import { execFileSync } from "node:child_process"
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
} from "node:fs"
import { dirname, join, resolve } from "node:path"
import { tmpdir } from "node:os"
import { fileURLToPath } from "node:url"
import { discoverSigntool } from "./sign-windows.mjs"
import { verifyArtifacts } from "./verify-signatures.mjs"

const SHARE_ROOT = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../native-share"
)

function requireFile(path) {
  if (
    !existsSync(path) ||
    !statSync(path).isFile() ||
    statSync(path).size === 0
  )
    throw new Error(`Native share bundle file is missing: ${path}`)
}

function readConfig(root) {
  const path = join(root, "config.json")
  requireFile(path)
  const config = JSON.parse(readFileSync(path, "utf8"))
  if (config.schemaVersion !== 1)
    throw new Error("Unsupported native share build configuration")
  return config
}

function verifyWindowsPackage(root) {
  const directory = mkdtempSync(join(tmpdir(), "iyw-share-verify-"))
  try {
    const manifest = join(directory, "receiver.manifest")
    execFileSync(
      join(dirname(discoverSigntool()), "mt.exe"),
      [
        `-inputresource:${join(root, "iyw-share-target.exe")};#1`,
        `-out:${manifest}`,
      ],
      { stdio: "pipe", windowsHide: true }
    )
    execFileSync(
      "powershell.exe",
      [
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        join(SHARE_ROOT, "windows/verify-package.ps1"),
        "-Directory",
        root,
        "-ExecutableManifest",
        manifest,
      ],
      {
        stdio: "inherit",
        windowsHide: true,
        env: Object.fromEntries(
          Object.entries(process.env).filter(
            ([name]) => name.toUpperCase() !== "PSMODULEPATH"
          )
        ),
      }
    )
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
}

export function verifyWindowsShare({ root, version, requireSigned = false }) {
  const config = readConfig(root)
  if (config.packageVersion !== `${version}.0`)
    throw new Error("Native share package version does not match the app")
  for (const name of [
    "iyw-share-target.exe",
    "register.ps1",
    "share-resources.ps1",
    "UPSTREAM-LICENSE",
  ])
    requireFile(join(root, name))
  if (!config.signed) {
    if (requireSigned)
      throw new Error(
        "Signed release is missing the native share identity package"
      )
    return
  }
  for (const name of [
    "IywAssistant.ShareTarget.msix",
    "resources.pri",
    "Assets/Square44x44Logo.png",
    "Assets/Square150x150Logo.png",
  ])
    requireFile(join(root, name))
  verifyWindowsPackage(root)
  verifyArtifacts([
    join(root, "iyw-share-target.exe"),
    join(root, "IywAssistant.ShareTarget.msix"),
  ])
}

export function verifyMacShare({ root, extension, version }) {
  const config = readConfig(root)
  requireFile(join(extension, "Contents/MacOS/IywAssistantShare"))
  const plist = join(extension, "Contents/Info.plist")
  requireFile(plist)
  const value = (key) =>
    execFileSync("/usr/libexec/PlistBuddy", ["-c", `Print :${key}`, plist], {
      encoding: "utf8",
    }).trim()
  if (
    value("CFBundleIdentifier") !== "app.iywclaw.share" ||
    value("CFBundleShortVersionString") !== version ||
    value("IywShareAppGroup") !== (config.appGroup || "")
  )
    throw new Error("macOS native share metadata does not match the app")
  execFileSync("codesign", ["--verify", "--strict", extension], {
    stdio: "inherit",
  })
}

export function verifyInstalledNativeShare({ appDirectory, target, version }) {
  if (target.includes("win7") || target.includes("linux")) return
  if (target.endsWith("-windows-msvc")) {
    verifyWindowsShare({
      root: join(appDirectory, "native-share"),
      version,
      requireSigned:
        process.env.IYW_CLAW_SIGN_MODE !== "none" &&
        !!process.env.IYW_CLAW_SIGN_MODE,
    })
  } else if (target.endsWith("-apple-darwin")) {
    verifyMacShare({
      root: resolve(appDirectory, "../Resources/native-share"),
      extension: resolve(appDirectory, "../PlugIns/IywAssistantShare.appex"),
      version,
    })
  }
}

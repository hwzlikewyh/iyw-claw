import { execFileSync } from "node:child_process"
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs"
import { basename, dirname, join, resolve } from "node:path"
import { tmpdir } from "node:os"
import { fileURLToPath } from "node:url"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..")
const CAPABILITY = "IYW_NATIVE_SHARE_MAIN_V1"

export function assertMainShareReceiver(file) {
  if (!readFileSync(file).includes(Buffer.from(CAPABILITY)))
    throw new Error(
      "Main application does not contain the built-in share receiver"
    )
}

export function prepareMainShareIdentity({
  file,
  signtool,
  env = process.env,
}) {
  if (basename(file).toLowerCase() !== "iyw-claw.exe") return
  const target = env.TAURI_TARGET_TRIPLE || env.CARGO_BUILD_TARGET || ""
  if (target.includes("win7") || file.includes("-win7-")) return
  const root = env.IYW_CLAW_BUILD_ROOT || ROOT
  const configPath = join(root, "src-tauri/native-share/resources/config.json")
  if (!existsSync(configPath))
    throw new Error("Native share build configuration is missing")
  const config = JSON.parse(readFileSync(configPath, "utf8"))
  if (!config.signed) return
  if (config.receiver !== "main" || !config.publisher)
    throw new Error("Native share must use the signed main application")
  const version = JSON.parse(
    readFileSync(join(root, "package.json"), "utf8")
  ).version
  if (config.packageVersion !== `${version}.0`)
    throw new Error("Native share version does not match the main application")
  assertMainShareReceiver(file)
  const directory = mkdtempSync(join(tmpdir(), "iyw-share-main-"))
  try {
    embedIdentity({
      file,
      signtool,
      root,
      directory,
      publisher: config.publisher,
    })
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
}

function embedIdentity({ file, signtool, root, directory, publisher }) {
  const mt = join(dirname(signtool), "mt.exe")
  const original = join(directory, "original.manifest")
  const manifest = join(directory, "main.manifest")
  const options = { windowsHide: true, stdio: "pipe", timeout: 30_000 }
  execFileSync(mt, [`-inputresource:${file};#1`, `-out:${original}`], options)
  const systemRoot = process.env.SystemRoot
  if (!systemRoot) throw new Error("Windows system directory is missing")
  const powershell = join(
    systemRoot,
    "System32/WindowsPowerShell/v1.0/powershell.exe"
  )
  const env = Object.fromEntries(
    Object.entries(process.env).filter(
      ([name]) => name.toUpperCase() !== "PSMODULEPATH"
    )
  )
  execFileSync(
    powershell,
    [
      "-NoProfile",
      "-NonInteractive",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      join(root, "src-tauri/native-share/windows/prepare-main-manifest.ps1"),
      "-Manifest",
      original,
      "-Publisher",
      publisher,
      "-Output",
      manifest,
    ],
    { ...options, env }
  )
  execFileSync(
    mt,
    ["-manifest", manifest, `-outputresource:${file};#1`],
    options
  )
  console.log(
    "[native-share] embedded share identity into the main application before signing"
  )
}

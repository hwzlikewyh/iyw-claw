import { copyFileSync, mkdirSync } from "node:fs"
import { basename, join } from "node:path"
import { execFileSync } from "node:child_process"
import { packageWindowsPayload } from "../../../src-tauri/scripts/windows-payload.mjs"
import { uploadDraftFile } from "./github-assets.mjs"

export async function publishWindowsPayload({ root, version, sourceCommit, context, assets }) {
  const installer = process.env.IYW_UNIFIED_INSTALLER
  if (!installer) throw new Error("verified direct installer is required for Windows payload release")
  const output = join(process.env.RUNNER_TEMP, `payload-${process.env.GITHUB_RUN_ID}-${process.env.GITHUB_RUN_ATTEMPT}`)
  const webview = join(process.env.LOCALAPPDATA, "tauri/MicrosoftEdgeWebview2Setup.exe")
  const powershellEnv = Object.fromEntries(Object.entries(process.env)
    .filter(([key]) => key.toUpperCase() !== "PSMODULEPATH"))
  execFileSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File",
    join(root, "src-tauri/scripts/verify-payload-inputs.ps1"), "-Installer", installer, "-WebView", webview,
    "-Application", join(root, "src-tauri/target/x86_64-pc-windows-msvc/release/iyw-claw.exe"),
    "-Helper", join(root, "src-tauri/binaries/iyw-environment-x86_64-pc-windows-msvc.exe")], { stdio: "inherit", windowsHide: true, env: powershellEnv })
  const archive = await packageWindowsPayload({ root, output, version, sourceCommit, installer, webview })
  mkdirSync(assets, { recursive: true })
  const bootstrapper = join(assets, `iyw-claw_${version}_x64-bootstrapper.exe`)
  copyFileSync(installer, bootstrapper)
  const verification = join(root, "src-tauri/target/x86_64-pc-windows-msvc/release/bundle/direct")
  mkdirSync(verification, { recursive: true })
  for (const file of [archive, bootstrapper]) {
    execFileSync(process.execPath, [join(root, "node_modules/@tauri-apps/cli/tauri.js"), "signer", "sign", file], { stdio: "inherit", windowsHide: true })
    await uploadDraftFile(context, file)
    await uploadDraftFile(context, `${file}.sig`)
    copyFileSync(file, join(verification, basename(file)))
    copyFileSync(`${file}.sig`, join(verification, `${basename(file)}.sig`))
  }
}

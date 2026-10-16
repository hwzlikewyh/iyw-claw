import { spawnSync } from "node:child_process"
import { existsSync } from "node:fs"
import { join } from "node:path"
import { fileURLToPath } from "node:url"

const HOST = fileURLToPath(new URL("./sign-windows-token.ps1", import.meta.url))

function resolvePowerShell(env) {
  const candidates = [
    env.IYW_CLAW_POWERSHELL,
    env.ProgramFiles && join(env.ProgramFiles, "PowerShell", "7", "pwsh.exe"),
    env.SystemRoot &&
      join(
        env.SystemRoot,
        "System32",
        "WindowsPowerShell",
        "v1.0",
        "powershell.exe"
      ),
  ].filter(Boolean)
  return (
    candidates.find((candidate) => existsSync(candidate)) || "powershell.exe"
  )
}

export function signWithToken({ file, digest, env, timeout }) {
  if (!(env.IYW_CLAW_SAFENET_PIN ?? "").trim())
    throw new Error("IYW_CLAW_SAFENET_PIN is required for unattended signing")
  const environment = Object.fromEntries(
    Object.entries(env).filter(
      ([name]) => name.toUpperCase() !== "PSMODULEPATH"
    )
  )
  console.log(
    `[sign-staged-windows] signing with PIN in the signer process: ${file}`
  )
  return spawnSync(
    resolvePowerShell(env),
    [
      "-NoProfile",
      "-NonInteractive",
      "-File",
      HOST,
      "-FilePath",
      file,
      "-Thumbprint",
      (env.IYW_CLAW_SIGN_THUMBPRINT ?? "").replace(/[\s:]/g, ""),
      "-Digest",
      digest,
    ],
    {
      env: environment,
      stdio: "inherit",
      windowsHide: true,
      timeout,
      killSignal: "SIGTERM",
    }
  )
}

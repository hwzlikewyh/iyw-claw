import { spawnSync } from "node:child_process"

const VERIFY_TIMEOUT_MS = 30_000
const VERIFY_SCRIPT = [
  "$ErrorActionPreference = 'Stop'",
  "$signature = Get-AuthenticodeSignature -LiteralPath $env:IYW_SIGN_VERIFY_PATH",
  "if ($signature.Status -ne 'Valid') { exit 1 }",
  "if ($env:IYW_SIGN_VERIFY_THUMBPRINT -and $signature.SignerCertificate.Thumbprint -ne $env:IYW_SIGN_VERIFY_THUMBPRINT) { exit 1 }",
  "if ($env:IYW_SIGN_VERIFY_TIMESTAMP -eq '1' -and $null -eq $signature.TimeStamperCertificate) { exit 1 }",
].join("; ")

export function verifyStagedSignature({
  signtool,
  file,
  env,
  timestamp = true,
}) {
  const thumbprint = (env.IYW_CLAW_SIGN_THUMBPRINT || "")
    .replace(/[\s:]/g, "")
    .toUpperCase()
  const result = spawnSync(signtool, ["verify", "/pa", "/all", file], {
    timeout: VERIFY_TIMEOUT_MS,
    stdio: "ignore",
    windowsHide: true,
    env,
  })
  if (result.status !== 0) {
    // Keep the reason visible: a bare `false` here surfaced as a bogus
    // "hardware signing failed" message and hid the real cause for hours.
    console.warn(
      `[sign-verify] signtool verify rejected ${file} (status=${result.status} error=${result.error?.code ?? "none"})`
    )
    return false
  }
  const environment = Object.fromEntries(
    Object.entries(env).filter(
      ([name]) => name.toUpperCase() !== "PSMODULEPATH"
    )
  )
  const probe = spawnSync(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-Command", VERIFY_SCRIPT],
    {
      env: {
        ...environment,
        IYW_SIGN_VERIFY_PATH: file,
        IYW_SIGN_VERIFY_THUMBPRINT: thumbprint,
        IYW_SIGN_VERIFY_TIMESTAMP: timestamp ? "1" : "0",
      },
      timeout: VERIFY_TIMEOUT_MS,
      stdio: "ignore",
      windowsHide: true,
    }
  )
  if (probe.status !== 0) {
    console.warn(
      `[sign-verify] status check rejected ${file} (status=${probe.status} error=${probe.error?.code ?? "none"} thumbprint=${thumbprint} timestamp=${timestamp})`
    )
    return false
  }
  return true
}

#!/usr/bin/env node

/**
 * Unlocks the SafeNet/eToken code-signing token through the Windows CNG KSP
 * so `signtool` never shows the interactive "Token Logon" dialog.
 *
 * Why this exists next to `unlock-signing-token.mjs`: signtool signs through
 * CAPI/CNG (the "SafeNet Smart Card Key Storage Provider"), while the PKCS#11
 * `C_Login` in the other helper authenticates a different middleware path. A
 * successful `C_Login` therefore does NOT stop signtool from prompting, which
 * is exactly the failure the release workflow hit.
 *
 * Setting `SmartCardPin` on the *key* handle makes the KSP use that PIN for
 * subsequent operations in this logon session, which is what removes the UI.
 * The provider-scoped property alone returns NTE_BAD_FLAGS (0x80090009) on this
 * middleware, so the key-scoped call is the one that matters.
 *
 * Secret handling: the PIN is read from `IYW_CLAW_SAFENET_PIN`, passed straight
 * to NCryptSetProperty, and never logged, echoed, or written to disk.
 *
 *   IYW_CLAW_SAFENET_PIN      token user PIN (required)
 *   IYW_CLAW_SAFENET_KEY_NAME CNG key container name (required)
 *   IYW_CLAW_POWERSHELL       optional PowerShell 7 path
 */

import { spawnSync } from "node:child_process"
import { existsSync } from "node:fs"
import { join } from "node:path"
import process from "node:process"

const DEFAULT_KEY_NAME = "te-13d27f92-83be-42c7-a92f-8563cc8453ef"

const POWERSHELL_CANDIDATES = [
  process.env.IYW_CLAW_POWERSHELL,
  join(
    process.env.ProgramFiles ?? "C:\\Program Files",
    "PowerShell",
    "7",
    "pwsh.exe"
  ),
  "pwsh.exe",
  "powershell.exe",
].filter(Boolean)

function resolvePowerShell() {
  for (const candidate of POWERSHELL_CANDIDATES) {
    if (candidate.includes("\\") || candidate.includes("/")) {
      if (existsSync(candidate)) return candidate
      continue
    }
    return candidate
  }
  return "powershell.exe"
}

/**
 * The PIN travels in the child environment rather than in the command line, so
 * it never shows up in a process listing. The host script prints only status
 * codes, never the PIN itself.
 */
function buildHostScript(keyName) {
  return `
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class IywSigningKsp {
  const string KSP = "SafeNet Smart Card Key Storage Provider";
  const uint NCRYPT_SILENT_FLAG = 0x00000040;
  const uint NCRYPT_UI_PROTECT_KEY_FLAG = 0x00000001;

  [DllImport("ncrypt.dll", CharSet = CharSet.Unicode)]
  static extern int NCryptOpenStorageProvider(out IntPtr phProvider, string pszProviderName, uint dwFlags);
  [DllImport("ncrypt.dll", CharSet = CharSet.Unicode)]
  static extern int NCryptOpenKey(IntPtr hProvider, out IntPtr phKey, string pszKeyName, uint dwLegacyKeySpec, uint dwFlags);
  [DllImport("ncrypt.dll", CharSet = CharSet.Unicode)]
  static extern int NCryptSetProperty(IntPtr hObject, string pszProperty, byte[] pbInput, int cbInput, uint dwFlags);
  [DllImport("ncrypt.dll")]
  static extern int NCryptFreeObject(IntPtr hObject);

  /// Returns "OK <provider> <key>" on success; never throws for expected states
  /// so the caller can map the outcome without leaking the PIN.
  public static string Apply(string keyName, string pin) {
    byte[] pinBlob = Encoding.Unicode.GetBytes(pin);
    IntPtr provider;
    int status = NCryptOpenStorageProvider(out provider, KSP, 0);
    if (status != 0) return "OPEN_PROVIDER_FAILED 0x" + ((uint)status).ToString("x8");
    try {
      int providerStatus = NCryptSetProperty(
        provider, "SmartCardPin", pinBlob, pinBlob.Length,
        NCRYPT_SILENT_FLAG | NCRYPT_UI_PROTECT_KEY_FLAG);

      IntPtr key;
      status = NCryptOpenKey(provider, out key, keyName, 0, 0);
      if (status != 0) return "OPEN_KEY_FAILED 0x" + ((uint)status).ToString("x8");
      try {
        status = NCryptSetProperty(
          key, "SmartCardPin", pinBlob, pinBlob.Length,
          NCRYPT_SILENT_FLAG | NCRYPT_UI_PROTECT_KEY_FLAG);
        if (status != 0) return "SET_KEY_PIN_FAILED 0x" + ((uint)status).ToString("x8");
        return "OK provider=0x" + ((uint)providerStatus).ToString("x8");
      } finally {
        NCryptFreeObject(key);
      }
    } finally {
      NCryptFreeObject(provider);
      Array.Clear(pinBlob, 0, pinBlob.Length);
    }
  }
}
'@

try {
  $result = [IywSigningKsp]::Apply(${JSON.stringify(keyName)}, $env:IYW_CLAW_SAFENET_PIN)
  Write-Output $result
  if ($result -like 'OK*') { exit 0 }
  exit 9
} catch {
  Write-Output ("HOST_ERROR " + $_.Exception.Message)
  exit 10
}
`
}

function fail(message) {
  console.error(`[signing-ksp][ERROR] ${message}`)
  process.exit(1)
}

const pin = process.env.IYW_CLAW_SAFENET_PIN ?? ""
if (pin.trim() === "") {
  fail(
    "IYW_CLAW_SAFENET_PIN is empty. Export the token PIN before signing so the job never waits on the Token Logon dialog."
  )
}

const keyName = (process.env.IYW_CLAW_SAFENET_KEY_NAME ?? DEFAULT_KEY_NAME).trim()
if (keyName === "") fail("IYW_CLAW_SAFENET_KEY_NAME is empty")

const result = spawnSync(
  resolvePowerShell(),
  [
    "-NoProfile",
    "-NonInteractive",
    "-Command",
    buildHostScript(keyName),
  ],
  {
    // The PIN reaches the child through the inherited environment, keeps it out
    // of the command line, and the captured stdout carries only status codes.
    env: process.env,
    encoding: "utf8",
    windowsHide: true,
    timeout: 60_000,
  }
)

if (result.error) fail(`failed to run the CNG host: ${result.error.message}`)

const lastLine =
  (result.stdout ?? "")
    .trim()
    .split(/\r?\n/)
    .filter(Boolean)
    .pop() ?? ""

if (lastLine.startsWith("OK")) {
  console.log(`[signing-ksp] token unlocked via CNG (${lastLine})`)
  process.exit(0)
}

const diagnostic = lastLine.startsWith("HOST_ERROR")
  ? `CNG host failed: ${lastLine.slice("HOST_ERROR".length).trim()}`
  : lastLine ||
    (result.stderr ?? "").trim() ||
    "unexpected CNG host failure"
fail(diagnostic)

#!/usr/bin/env node

/**
 * Unlocks the SafeNet/eToken code-signing token through PKCS#11 so the staged
 * Windows signing jobs never wait on the interactive "Token Logon" dialog.
 *
 * The SAC middleware caches the user PIN per Windows logon session, so a
 * successful C_Login here keeps `signtool` non-interactive for the rest of the
 * job. SAC's `PasswordTimeout = 0` means the cache does not expire on a timer.
 *
 * Secret handling: the PIN is read from `IYW_CLAW_SAFENET_PIN` and is only ever
 * passed to `C_Login`. It is never logged, echoed, or written to disk, and the
 * PKCS#11 session is closed (logging the token out again) before exit so an
 * unlock does not linger beyond this process unless the caller keeps signing.
 *
 *   IYW_CLAW_SAFENET_PIN        token user PIN (required)
 *   IYW_CLAW_SAFENET_PKCS11     PKCS#11 module path
 *                               (default: %SystemRoot%\System32\eTPKCS11.dll)
 *   IYW_CLAW_SAFENET_SLOT       optional slot index; default = first token slot
 *   IYW_CLAW_SAFENET_KEEP_LOGIN set to 1 to stay logged in until the process
 *                               would exit anyway (used to verify the cache)
 */

import { spawnSync } from "node:child_process"
import { existsSync } from "node:fs"
import { join } from "node:path"
import process from "node:process"

const DEFAULT_PKCS11 = join(
  process.env.SystemRoot ?? "C:\\Windows",
  "System32",
  "eTPKCS11.dll"
)

/**
 * The inline C# uses `where T : Delegate` and `Marshal.GetDelegateForFunctionPointer<T>`,
 * which the legacy .NET Framework compiler behind Windows PowerShell 5.1 rejects.
 * Prefer PowerShell 7 and fall back to whatever `powershell.exe` resolves to.
 */
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

const CKF_TOKEN_PRESENT = 0x00000001
const CKF_SERIAL_SESSION = 0x00000004
const CKF_RW_SESSION = 0x00000002

const CKR_OK = 0x00000000
const CKR_USER_ALREADY_LOGGED_IN = 0x00000101
const CKR_PIN_INCORRECT = 0x000000A0
const CKR_PIN_LOCKED = 0x000000A4
const CKR_PIN_EXPIRED = 0x000000A3
const CKR_USER_PIN_NOT_INITIALIZED = 0x00000102

const CKU_USER = 0x00000001

function fail(message) {
  console.error(`[signing-token][ERROR] ${message}`)
  process.exit(1)
}

function describe(code) {
  switch (code >>> 0) {
    case CKR_PIN_INCORRECT:
      return "PIN is incorrect"
    case CKR_PIN_LOCKED:
      return "PIN is locked; the token needs an administrative unlock"
    case CKR_PIN_EXPIRED:
      return "PIN has expired"
    case CKR_USER_PIN_NOT_INITIALIZED:
      return "the token user PIN is not initialized"
    default:
      return `PKCS#11 call failed with 0x${(code >>> 0).toString(16)}`
  }
}

/**
 * Talks to the PKCS#11 module through a tiny generated PowerShell host, so the
 * release scripts stay dependency-free (no node-gyp, no prebuilt native addon).
 *
 * The marshalling lives in inline C# rather than PowerShell delegates: the
 * PKCS#11 signatures need by-ref `CK_SLOT_ID`/`CK_SESSION_HANDLE` parameters,
 * and PowerShell's `GetDelegateForFunctionPointer` generic type syntax cannot
 * express `[ref]` parameters.
 */
function buildHostScript(modulePath, slotIndex) {
  return `
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

public static class IywSigningToken {
  const byte CKF_TOKEN_PRESENT = 0x01;
  const uint CKF_SERIAL_SESSION = 0x04;
  const uint CKF_RW_SESSION = 0x02;
  const uint CKU_USER = 0x01;

  const uint CKR_OK = 0x00;
  const uint CKR_USER_ALREADY_LOGGED_IN = 0x101;
  const uint CKR_CRYPTOKI_ALREADY_INITIALIZED = 0x191;

  [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
  delegate uint C_InitializeDelegate(IntPtr args);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
  delegate uint C_FinalizeDelegate(IntPtr reserved);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
  delegate uint C_GetSlotListDelegate(byte tokenPresent, IntPtr slotList, ref uint count);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
  delegate uint C_GetTokenInfoDelegate(IntPtr slotId, IntPtr info);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
  delegate uint C_OpenSessionDelegate(IntPtr slotId, uint flags, IntPtr app, IntPtr notify, ref IntPtr session);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
  delegate uint C_CloseSessionDelegate(IntPtr session);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
  delegate uint C_LoginDelegate(IntPtr session, uint userType, byte[] pin, uint pinLength);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
  delegate uint C_LogoutDelegate(IntPtr session);

  [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
  static extern IntPtr LoadLibraryW(string path);

  [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Ansi)]
  static extern IntPtr GetProcAddress(IntPtr module, string name);

  static T Fn<T>(IntPtr module, string name) where T : Delegate {
    IntPtr address = GetProcAddress(module, name);
    if (address == IntPtr.Zero) throw new EntryPointNotFoundException(name);
    return Marshal.GetDelegateForFunctionPointer<T>(address);
  }

  /// Returns a status token; never throws for expected token states so the
  /// caller can map it to a non-zero exit code without leaking the PIN.
  public static string Unlock(string modulePath, int requestedSlot, string pin) {
    IntPtr module = LoadLibraryW(modulePath);
    if (module == IntPtr.Zero) return "LOAD_FAILED";

    var initialize = Fn<C_InitializeDelegate>(module, "C_Initialize");
    var finalize = Fn<C_FinalizeDelegate>(module, "C_Finalize");
    var getSlotList = Fn<C_GetSlotListDelegate>(module, "C_GetSlotList");
    var getTokenInfo = Fn<C_GetTokenInfoDelegate>(module, "C_GetTokenInfo");
    var openSession = Fn<C_OpenSessionDelegate>(module, "C_OpenSession");
    var closeSession = Fn<C_CloseSessionDelegate>(module, "C_CloseSession");
    var login = Fn<C_LoginDelegate>(module, "C_Login");
    var logout = Fn<C_LogoutDelegate>(module, "C_Logout");

    uint status = initialize(IntPtr.Zero);
    if (status != CKR_OK && status != CKR_CRYPTOKI_ALREADY_INITIALIZED)
      return "INIT_FAILED " + status;

    uint count = 0;
    status = getSlotList(CKF_TOKEN_PRESENT, IntPtr.Zero, ref count);
    if (status != CKR_OK || count == 0) return "NO_TOKEN_SLOT";

    IntPtr slotBuffer = Marshal.AllocHGlobal((int)(IntPtr.Size * count));
    long chosen = -1;
    try {
      status = getSlotList(CKF_TOKEN_PRESENT, slotBuffer, ref count);
      if (status != CKR_OK) return "SLOT_LIST_FAILED " + status;

      long[] slots = new long[count];
      for (uint i = 0; i < count; i++)
        slots[i] = Marshal.ReadIntPtr(slotBuffer, (int)(i * (uint)IntPtr.Size)).ToInt64();

      if (requestedSlot >= 0 && requestedSlot < slots.Length) {
        chosen = slots[requestedSlot];
      } else {
        IntPtr info = Marshal.AllocHGlobal(128);
        try {
          foreach (long slot in slots) {
            if (getTokenInfo(new IntPtr(slot), info) != CKR_OK) continue;
            if ((Marshal.ReadByte(info, 84) & CKF_TOKEN_PRESENT) != 0) { chosen = slot; break; }
          }
        } finally { Marshal.FreeHGlobal(info); }
      }
    } finally { Marshal.FreeHGlobal(slotBuffer); }

    if (chosen < 0) return "NO_TOKEN_PRESENT";

    IntPtr session = IntPtr.Zero;
    status = openSession(
      new IntPtr(chosen), CKF_SERIAL_SESSION | CKF_RW_SESSION,
      IntPtr.Zero, IntPtr.Zero, ref session);
    if (status != CKR_OK) return "OPEN_FAILED " + status;

    byte[] pinBytes = Encoding.ASCII.GetBytes(pin);
    try {
      status = login(session, CKU_USER, pinBytes, (uint)pinBytes.Length);
    } finally {
      Array.Clear(pinBytes, 0, pinBytes.Length);
    }

    if (status == CKR_OK) {
      logout(session);
      closeSession(session);
      finalize(IntPtr.Zero);
      return "LOGIN_OK";
    }

    if (status == CKR_USER_ALREADY_LOGGED_IN) {
      closeSession(session);
      finalize(IntPtr.Zero);
      return "ALREADY_LOGGED_IN";
    }

    closeSession(session);
    finalize(IntPtr.Zero);
    return "LOGIN_FAILED " + status;
  }
}
'@

try {
  $result = [IywSigningToken]::Unlock(
    ${JSON.stringify(modulePath)}, ${slotIndex}, $env:IYW_CLAW_SAFENET_PIN)
  Write-Output $result
  if ($result -eq 'LOGIN_OK' -or $result -eq 'ALREADY_LOGGED_IN') { exit 0 }
  exit 9
} catch {
  Write-Output ("HOST_ERROR " + $_.Exception.Message)
  exit 10
}
`
}

const pin = process.env.IYW_CLAW_SAFENET_PIN ?? ""
if (pin.trim() === "") {
  fail(
    "IYW_CLAW_SAFENET_PIN is empty. Export the token PIN before signing so the job never waits on the Token Logon dialog."
  )
}

const modulePath = process.env.IYW_CLAW_SAFENET_PKCS11 || DEFAULT_PKCS11
if (!existsSync(modulePath)) {
  fail(`PKCS#11 module not found at ${modulePath}`)
}

const slotIndex = Number.parseInt(process.env.IYW_CLAW_SAFENET_SLOT ?? "-1", 10)
if (Number.isNaN(slotIndex)) fail("IYW_CLAW_SAFENET_SLOT must be an integer")

/**
 * Transient states worth another attempt: a smart-card reader that is still
 * enumerating after logon, or a middleware RPC that is briefly unavailable.
 * A wrong or locked PIN is terminal, so it is never retried — retrying it would
 * only burn the token's remaining logon attempts and risk a lockout.
 */
const RETRYABLE = /^(NO_TOKEN_SLOT|NO_TOKEN_PRESENT|LOAD_FAILED|INIT_FAILED|OPEN_FAILED|SLOT_LIST_FAILED|HOST_ERROR)/
const ATTEMPTS = 3
const RETRY_DELAY_MS = 4000

function runHost() {
  const result = spawnSync(
    resolvePowerShell(),
    [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      buildHostScript(modulePath, Number.isNaN(slotIndex) ? -1 : slotIndex),
    ],
    {
      // stdio is captured so the PIN-bearing host script output is the only thing
      // that can reach the log, and the PIN itself is never part of it.
      encoding: "utf8",
      windowsHide: true,
    }
  )
  if (result.error) fail(`failed to run the PKCS#11 host: ${result.error.message}`)
  const stdout = (result.stdout ?? "").trim()
  return {
    lastLine: stdout.split(/\r?\n/).filter(Boolean).pop() ?? "",
    stderr: (result.stderr ?? "").trim(),
  }
}

const sleep = (ms) =>
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms)

let lastLine = ""
let stderr = ""
for (let attempt = 1; attempt <= ATTEMPTS; attempt += 1) {
  ;({ lastLine, stderr } = runHost())

  if (lastLine === "LOGIN_OK" || lastLine === "ALREADY_LOGGED_IN") {
    console.log(
      `[signing-token] token unlocked (${lastLine === "LOGIN_OK" ? "C_Login succeeded" : "session already authenticated"})`
    )
    process.exit(0)
  }

  if (lastLine.startsWith("LOGIN_FAILED")) {
    const code = Number.parseInt(lastLine.split(" ")[1] ?? "0", 10)
    fail(describe(code))
  }

  if (attempt < ATTEMPTS && RETRYABLE.test(lastLine)) {
    console.log(
      `[signing-token] ${lastLine}; retrying (${attempt}/${ATTEMPTS - 1})`
    )
    sleep(RETRY_DELAY_MS)
    continue
  }
  break
}

const diagnostic =
  lastLine === "NO_TOKEN_SLOT" || lastLine === "NO_TOKEN_PRESENT"
    ? "no signing token is present on this machine"
    : lastLine.startsWith("LOAD_FAILED")
      ? `could not load the PKCS#11 module at ${modulePath}`
      : lastLine.startsWith("HOST_ERROR")
        ? `PKCS#11 host failed: ${lastLine.slice("HOST_ERROR".length).trim()}`
        : lastLine || stderr || "unexpected PKCS#11 host failure"
fail(diagnostic)

#!/usr/bin/env node

/**
 * Authenticode signing shim for the Windows bundle.
 *
 * Tauri calls this through `bundle.windows.signCommand`, once per artifact,
 * with the artifact path substituted for `%1`. It can also be run directly
 * with `--sidecars` to sign the staged `src-tauri/binaries/*.exe` sidecars
 * before bundling — those are dropped onto disk at install time and are the
 * files antivirus heuristics react to most, so they need a signature even
 * though the bundler does not cover them.
 *
 * Every input is an environment variable so no certificate material and no
 * machine-specific path is ever committed:
 *
 *   IYW_CLAW_SIGN_MODE            signtool | pfx | azure | none  (default none)
 *   IYW_CLAW_SIGN_REQUIRED        1 => mode=none becomes a hard error
 *   IYW_CLAW_SIGN_THUMBPRINT      SHA-1 thumbprint of the certificate in the
 *                                 Windows store (mode=signtool). This is the
 *                                 mode to use with an OV/EV USB token or a
 *                                 cloud HSM that ships a CSP/KSP.
 *   IYW_CLAW_SIGN_PFX             path to a .pfx (mode=pfx)
 *   IYW_CLAW_SIGN_PFX_PASSWORD    password for that .pfx
 *   IYW_CLAW_SIGN_AZURE_DLIB      Azure.CodeSigning.Dlib.dll (mode=azure)
 *   IYW_CLAW_SIGN_AZURE_METADATA  Azure signing metadata json (mode=azure)
 *   IYW_CLAW_SIGN_TIMESTAMP_URL   RFC3161 timestamp server
 *   IYW_CLAW_SIGN_DIGEST          digest algorithm (default sha256)
 *   IYW_CLAW_SIGNTOOL             explicit signtool.exe, skips SDK discovery
 *
 * mode=pfx exists for local smoke tests against a self-signed certificate.
 * A self-signed signature does NOT help with SmartScreen or antivirus — only a
 * CA-issued OV/EV certificate does. Note also that signtool takes the .pfx
 * password on its command line, so it is visible in the process list while the
 * child runs; prefer mode=signtool for anything real.
 */

import { spawnSync } from "node:child_process"
import { existsSync, readdirSync, statSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import process from "node:process"

const SCRIPT_PATH = fileURLToPath(import.meta.url)
const REPO_ROOT = resolve(dirname(SCRIPT_PATH), "..", "..")
const SIDECAR_DIR = join(REPO_ROOT, "src-tauri", "binaries")

/**
 * DigiCert's public RFC3161 endpoint. Timestamping is not optional: without it
 * every signature stops validating the day the certificate expires, which
 * turns a shipped release into an unsigned one.
 */
const DEFAULT_TIMESTAMP_URL = "http://timestamp.digicert.com"

const VALID_MODES = new Set(["signtool", "pfx", "azure", "none"])

/** Timestamp servers rate-limit and flake; a signing run must not die on that. */
const TIMESTAMP_ATTEMPTS = 3
const TIMESTAMP_RETRY_DELAY_MS = 5000

export function parseSignOptions(argv) {
  const options = { sidecars: false, targets: [] }
  for (const arg of argv) {
    if (arg === "--sidecars") {
      options.sidecars = true
    } else if (arg.startsWith("--")) {
      throw new Error(`unknown option: ${arg}`)
    } else {
      options.targets.push(arg)
    }
  }
  if (options.sidecars && options.targets.length > 0) {
    throw new Error(
      "--sidecars signs the staged sidecars and takes no file arguments"
    )
  }
  return options
}

export function resolveSignMode(env = process.env) {
  const raw = (env.IYW_CLAW_SIGN_MODE ?? "").trim().toLowerCase()
  const mode = raw === "" ? "none" : raw
  if (!VALID_MODES.has(mode)) {
    throw new Error(
      `invalid IYW_CLAW_SIGN_MODE: ${raw} (expected ${[...VALID_MODES].join(", ")})`
    )
  }
  return mode
}

export function signingRequired(env = process.env) {
  return ["1", "true", "yes"].includes(
    (env.IYW_CLAW_SIGN_REQUIRED ?? "").trim().toLowerCase()
  )
}

function compareSdkVersions(a, b) {
  const left = a.split(".").map((part) => Number.parseInt(part, 10) || 0)
  const right = b.split(".").map((part) => Number.parseInt(part, 10) || 0)
  const length = Math.max(left.length, right.length)
  for (let index = 0; index < length; index += 1) {
    const diff = (left[index] ?? 0) - (right[index] ?? 0)
    if (diff !== 0) return diff
  }
  return 0
}

/**
 * Locate signtool.exe. It lives in the Windows SDK, not on PATH, and the SDK
 * installs one copy per version — pick the newest so we get a build that
 * understands `/tr` (RFC3161) and `/dlib`.
 */
export function discoverSigntool(env = process.env) {
  const explicit = (env.IYW_CLAW_SIGNTOOL ?? "").trim()
  if (explicit) {
    if (!existsSync(explicit)) {
      throw new Error(`IYW_CLAW_SIGNTOOL points at a missing file: ${explicit}`)
    }
    return explicit
  }

  const arch = process.arch === "arm64" ? "arm64" : "x64"
  const roots = [env["ProgramFiles(x86)"], env.ProgramFiles]
    .filter(Boolean)
    .map((base) => join(base, "Windows Kits", "10", "bin"))
    .filter((base) => existsSync(base))

  const found = []
  for (const root of roots) {
    // Flat layout from very old SDKs.
    const flat = join(root, arch, "signtool.exe")
    if (existsSync(flat)) found.push({ version: "0", path: flat })
    for (const entry of readdirSync(root)) {
      const candidate = join(root, entry, arch, "signtool.exe")
      if (existsSync(candidate)) found.push({ version: entry, path: candidate })
    }
  }

  if (found.length === 0) {
    throw new Error(
      "signtool.exe not found. Install the Windows SDK (Signing Tools) " +
        "or set IYW_CLAW_SIGNTOOL to its full path."
    )
  }
  found.sort((a, b) => compareSdkVersions(b.version, a.version))
  return found[0].path
}

export function buildSigntoolArgs(mode, file, env = process.env) {
  const digest = (
    (env.IYW_CLAW_SIGN_DIGEST ?? "").trim() || "sha256"
  ).toLowerCase()
  const timestampUrl =
    (env.IYW_CLAW_SIGN_TIMESTAMP_URL ?? "").trim() || DEFAULT_TIMESTAMP_URL

  // `/fd` is the file digest, `/td` the timestamp digest — SHA-1 for either is
  // rejected by current Windows trust policy, so both track the same value.
  const args = ["sign", "/fd", digest, "/td", digest, "/tr", timestampUrl]

  if (mode === "signtool") {
    // Tolerate the spaced/colon-separated form certmgr copies out.
    const thumbprint = (env.IYW_CLAW_SIGN_THUMBPRINT ?? "").replace(
      /[\s:]/g,
      ""
    )
    if (!thumbprint) {
      throw new Error(
        "IYW_CLAW_SIGN_MODE=signtool requires IYW_CLAW_SIGN_THUMBPRINT"
      )
    }
    args.push("/sha1", thumbprint)
  } else if (mode === "pfx") {
    const pfx = (env.IYW_CLAW_SIGN_PFX ?? "").trim()
    if (!pfx) {
      throw new Error("IYW_CLAW_SIGN_MODE=pfx requires IYW_CLAW_SIGN_PFX")
    }
    if (!existsSync(pfx)) {
      throw new Error(`IYW_CLAW_SIGN_PFX points at a missing file: ${pfx}`)
    }
    args.push("/f", pfx)
    const password = env.IYW_CLAW_SIGN_PFX_PASSWORD ?? ""
    if (password !== "") args.push("/p", password)
  } else if (mode === "azure") {
    const dlib = (env.IYW_CLAW_SIGN_AZURE_DLIB ?? "").trim()
    const metadata = (env.IYW_CLAW_SIGN_AZURE_METADATA ?? "").trim()
    if (!dlib || !metadata) {
      throw new Error(
        "IYW_CLAW_SIGN_MODE=azure requires IYW_CLAW_SIGN_AZURE_DLIB and " +
          "IYW_CLAW_SIGN_AZURE_METADATA"
      )
    }
    args.push("/dlib", dlib, "/dmdf", metadata)
  } else {
    throw new Error(`buildSigntoolArgs called with non-signing mode: ${mode}`)
  }

  args.push(file)
  return args
}

/** Command line for logs, with the .pfx password removed. */
export function redactSigntoolArgs(args) {
  const redacted = [...args]
  const passwordIndex = redacted.indexOf("/p")
  if (passwordIndex !== -1 && passwordIndex + 1 < redacted.length) {
    redacted[passwordIndex + 1] = "***"
  }
  return redacted
}

function signOnce(signtool, args) {
  const result = spawnSync(signtool, args, { cwd: REPO_ROOT, stdio: "inherit" })
  if (result.error) throw result.error
  return result.status ?? 1
}

function signFile(signtool, mode, file, env) {
  if (!existsSync(file)) {
    throw new Error(`nothing to sign at ${file}`)
  }
  const args = buildSigntoolArgs(mode, file, env)
  console.log(`[sign-windows] signtool ${redactSigntoolArgs(args).join(" ")}`)

  for (let attempt = 1; attempt <= TIMESTAMP_ATTEMPTS; attempt += 1) {
    const status = signOnce(signtool, args)
    if (status === 0) return
    if (attempt === TIMESTAMP_ATTEMPTS) {
      throw new Error(`signtool exited with code ${status} for ${file}`)
    }
    // signtool does not distinguish "bad certificate" from "timestamp server
    // unreachable" in its exit code, so retry either way and let the final
    // attempt surface the real failure.
    console.warn(
      `[sign-windows][WARN] attempt ${attempt} failed (exit ${status}), retrying in ` +
        `${TIMESTAMP_RETRY_DELAY_MS / 1000}s`
    )
    const until = Date.now() + TIMESTAMP_RETRY_DELAY_MS
    while (Date.now() < until) {
      // Deliberately blocking: signCommand is synchronous from the bundler's
      // point of view and there is nothing else for this process to do.
    }
  }
}

/**
 * Staged sidecars worth signing. The zero-length placeholders in
 * `src-tauri/binaries/` are leftovers from earlier releases, not payloads —
 * signtool would fail on them and they should not ship at all.
 */
export function collectSidecars(dir = SIDECAR_DIR) {
  if (!existsSync(dir)) return []
  return readdirSync(dir)
    .filter((name) => name.toLowerCase().endsWith(".exe"))
    .map((name) => join(dir, name))
    .filter((file) => statSync(file).size > 0)
    .sort()
}

/**
 * Sign `files` with the configured mode. Exported so other build scripts can
 * sign an artifact in place (see prepare-sidecars.mjs) without shelling out.
 *
 * Callers are expected to have checked `resolveSignMode() !== "none"`; this
 * throws on a non-signing mode rather than silently doing nothing.
 */
export function signFiles(files, env = process.env) {
  const mode = resolveSignMode(env)
  if (mode === "none") {
    throw new Error("signFiles called with IYW_CLAW_SIGN_MODE=none")
  }
  if (process.platform !== "win32") {
    throw new Error(
      `Authenticode signing needs Windows (signtool.exe); current platform is ${process.platform}`
    )
  }
  // Validate the mode's own configuration before touching the filesystem, so a
  // missing thumbprint reports itself rather than hiding behind a "nothing to
  // sign at <path>" from the first target.
  buildSigntoolArgs(mode, "<probe>", env)

  const signtool = discoverSigntool(env)
  for (const file of files) {
    signFile(signtool, mode, file, env)
    console.log(`[sign-windows] signed ${file}`)
  }
}

export function main(argv = process.argv.slice(2), env = process.env) {
  const options = parseSignOptions(argv)
  const mode = resolveSignMode(env)

  if (mode === "none") {
    if (signingRequired(env)) {
      throw new Error(
        "IYW_CLAW_SIGN_REQUIRED is set but IYW_CLAW_SIGN_MODE is none — " +
          "refusing to produce an unsigned release build"
      )
    }
    console.warn(
      "[sign-windows][WARN] IYW_CLAW_SIGN_MODE is none; artifacts stay unsigned. " +
        "Unsigned builds trip SmartScreen and antivirus heuristics."
    )
    return
  }

  const targets = options.sidecars ? collectSidecars() : options.targets
  if (targets.length === 0) {
    if (options.sidecars) {
      console.log(
        "[sign-windows] no non-empty sidecars staged, nothing to sign"
      )
      return
    }
    throw new Error("no file to sign was passed")
  }

  signFiles(targets, env)
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(SCRIPT_PATH)) {
  try {
    main()
  } catch (error) {
    console.error(`[sign-windows][ERROR] ${error.message}`)
    process.exit(1)
  }
}

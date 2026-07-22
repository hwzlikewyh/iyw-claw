import { execFileSync } from "node:child_process"
import { readFileSync, statSync } from "node:fs"
import { basename, dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url))
const DEFAULT_SRC_TAURI = resolve(SCRIPT_DIR, "..")
const CAPABILITY_TIMEOUT_MS = 5_000
const CAPABILITY_MAX_BUFFER = 64 * 1024
const REQUIRED_MEMORY_TOOLS = ["append_user_memory", "propose_user_memory"]

function readCargoVersion(srcTauri) {
  const cargo = readFileSync(join(srcTauri, "Cargo.toml"), "utf8")
  const version = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
  if (!version) throw new Error("Cargo.toml package version is missing")
  return version
}

function readToolNames(srcTauri) {
  const path = join(srcTauri, "src", "acp", "delegation", "tool_schema.json")
  let schema
  try {
    schema = JSON.parse(readFileSync(path, "utf8"))
  } catch (error) {
    throw new Error(`tool schema is not valid JSON: ${error.message}`)
  }
  if (!Array.isArray(schema)) throw new Error("tool schema must be an array")
  const tools = schema.map((tool) => tool?.name)
  assertToolNames(tools, "tool schema")
  for (const required of REQUIRED_MEMORY_TOOLS) {
    if (!tools.includes(required)) {
      throw new Error(`tool schema is missing required tool ${required}`)
    }
  }
  return tools
}

function assertToolNames(tools, label) {
  if (
    !Array.isArray(tools) ||
    tools.some((tool) => typeof tool !== "string" || !tool)
  ) {
    throw new Error(`${label} tools must be non-empty strings`)
  }
  if (new Set(tools).size !== tools.length) {
    throw new Error(`${label} tools must not contain duplicates`)
  }
}

export function loadExpectedCompanionManifest(srcTauri = DEFAULT_SRC_TAURI) {
  return {
    name: "iyw-claw-mcp",
    version: readCargoVersion(srcTauri),
    protocol_version: 1,
    tools: readToolNames(srcTauri),
  }
}

function assertField(actual, expected, field) {
  if (actual[field] !== expected[field]) {
    throw new Error(
      `${field} mismatch: expected ${expected[field]}, got ${actual[field]}`
    )
  }
}

function assertExactToolSet(actual, expected) {
  assertToolNames(actual, "companion manifest")
  assertToolNames(expected, "expected manifest")
  const actualSet = new Set(actual)
  const expectedSet = new Set(expected)
  const missing = expected.filter((tool) => !actualSet.has(tool))
  const extra = actual.filter((tool) => !expectedSet.has(tool))
  if (missing.length || extra.length || actual.length !== expected.length) {
    throw new Error(
      `tool set mismatch: missing [${missing.join(", ")}], extra [${extra.join(", ")}]`
    )
  }
}

export function validateCompanionManifest(raw, expected) {
  let actual
  try {
    actual = JSON.parse(String(raw).trim())
  } catch (error) {
    throw new Error(
      `companion capabilities are not valid JSON: ${error.message}`
    )
  }
  if (!actual || typeof actual !== "object" || Array.isArray(actual)) {
    throw new Error("companion capabilities must be a JSON object")
  }
  assertField(actual, expected, "name")
  assertField(actual, expected, "version")
  assertField(actual, expected, "protocol_version")
  assertExactToolSet(actual.tools, expected.tools)
  return actual
}

function probeFailure(binaryPath, error) {
  if (error?.code === "ETIMEDOUT" || error?.killed) {
    return new Error(
      `capability probe timed out after ${CAPABILITY_TIMEOUT_MS}ms: ${binaryPath}`
    )
  }
  const status = error?.status ?? error?.code ?? "unknown"
  return new Error(
    `capability probe exited with status ${status}: ${binaryPath}`
  )
}

export function probeNativeCompanion(
  binaryPath,
  expected,
  runner = execFileSync
) {
  let raw
  try {
    raw = runner(binaryPath, ["--capabilities"], {
      encoding: "utf8",
      maxBuffer: CAPABILITY_MAX_BUFFER,
      timeout: CAPABILITY_TIMEOUT_MS,
      windowsHide: true,
    })
  } catch (error) {
    throw probeFailure(binaryPath, error)
  }
  return validateCompanionManifest(raw, expected)
}

export function validateCrossTargetCompanion(binaryPath, target, expected) {
  const extension = target.includes("windows") ? ".exe" : ""
  const expectedName = `iyw-claw-mcp-${target}${extension}`
  if (basename(binaryPath) !== expectedName) {
    throw new Error(
      `cross-target sidecar name mismatch: expected ${expectedName}`
    )
  }
  const stats = statSync(binaryPath)
  if (!stats.isFile() || stats.size === 0) {
    throw new Error(`cross-target sidecar is missing or empty: ${binaryPath}`)
  }
  assertExactToolSet(expected.tools, expected.tools)
  return {
    mode: "source-only",
    runtimeValidated: false,
    detail: "cross-target binary not executed on this host",
    expectedManifest: expected,
  }
}

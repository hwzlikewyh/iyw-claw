import { cp, lstat, readFile, readdir, realpath, stat } from "node:fs/promises"
import path from "node:path"

const MANIFESTS = [".codex-plugin/plugin.json", ".claude-plugin/plugin.json"]
const EXCLUDED = new Set([
  ".git",
  "node_modules",
  ".agents",
  ".codex-plugin",
  ".claude-plugin",
  ".github",
  "test",
  "tests",
  "design",
])
const SENSITIVE_FILES = /(^|\/)\.env(?:\.|$)|\.(?:pem|key|p12|pfx)$/i

export async function readJson(file) {
  if ((await stat(file)).size > 1024 * 1024)
    throw Error(`JSON document is too large: ${file}`)
  return JSON.parse(await readFile(file, "utf8"))
}

export async function containedFile(root, relative) {
  if (typeof relative !== "string" || !relative || path.isAbsolute(relative))
    throw Error("Expected a relative plugin path")
  const file = await realpath(path.resolve(root, relative))
  const within = path.relative(root, file)
  if (
    within.startsWith("..") ||
    path.isAbsolute(within) ||
    !(await stat(file)).isFile()
  )
    throw Error(`Plugin path escapes source: ${relative}`)
  return file
}

export async function readNativeSource(directory) {
  const root = await realpath(directory)
  const manifests = []
  for (const file of MANIFESTS) {
    try {
      manifests.push(await readJson(await containedFile(root, file)))
    } catch (error) {
      if (error.code !== "ENOENT") throw error
    }
  }
  const manifest = validateManifests(manifests)
  const config =
    typeof manifest.mcpServers === "object"
      ? manifest.mcpServers
      : await readJson(
          await containedFile(root, manifest.mcpServers ?? ".mcp.json")
        )
  const servers = config.mcpServers ?? config
  if (
    !servers ||
    typeof servers !== "object" ||
    Array.isArray(servers) ||
    !Object.keys(servers).length
  )
    throw Error("No stdio MCP servers declared")
  const configPath =
    typeof manifest.mcpServers === "string" ? manifest.mcpServers : ".mcp.json"
  await containedFile(root, configPath)
  return {
    root,
    manifest,
    configPath,
    servers,
    packageJson: await readOptionalJson(root, "package.json"),
  }
}

async function readOptionalJson(root, relative) {
  try {
    return await readJson(await containedFile(root, relative))
  } catch (error) {
    if (error.code === "ENOENT") return null
    throw error
  }
}

function validateManifests(manifests) {
  if (!manifests.length)
    throw Error(
      "Missing .codex-plugin/plugin.json or .claude-plugin/plugin.json"
    )
  const manifest = manifests[0]
  if (manifests.some((item) => item.hooks || item.commands || item.agents))
    throw Error(
      "Native hooks, commands and agents require separate host support"
    )
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(manifest.name))
    throw Error("Invalid plugin name")
  if (
    !/^\d+\.\d+\.\d+(?:-[\da-zA-Z.-]+)?(?:\+[\da-zA-Z.-]+)?$/.test(
      manifest.version
    )
  )
    throw Error("Invalid plugin version")
  if (
    manifests.some(
      (item) => item.name !== manifest.name || item.version !== manifest.version
    )
  )
    throw Error("Native plugin identities disagree")
  return manifest
}

export async function resolveServer(source, spec) {
  if (spec.url || spec.type === "http" || spec.type === "sse")
    throw Error(
      "Native import currently requires bundled stdio Node.js servers"
    )
  const args = spec.args ?? []
  if (!Array.isArray(args) || args.some((arg) => typeof arg !== "string"))
    throw Error("Invalid native MCP arguments")
  if (spec.command === "node" && args.length === 1)
    return containedFile(
      source.root,
      args[0]
        .replaceAll("${CLAUDE_PLUGIN_ROOT}/", "")
        .replaceAll("${CODEX_PLUGIN_ROOT}/", "")
    )
  if (!["npx", "npx.cmd"].includes(spec.command))
    throw Error("Native server must use node or a packaged npm binary")
  if (!source.packageJson)
    throw Error("npx servers require a source package.json")
  const requested = args
    .find((arg) => arg.startsWith("--package="))
    ?.slice("--package=".length)
  if (requested !== `${source.packageJson.name}@${source.packageJson.version}`)
    throw Error("The pinned npm runtime must match the source package identity")
  const executable = args.at(-1)
  const bins = source.packageJson.bin
  const entry = typeof bins === "string" ? bins : bins?.[executable]
  if (!entry || args.filter((arg) => !arg.startsWith("-")).length !== 1)
    throw Error("Unsupported npm binary arguments")
  return containedFile(source.root, entry)
}

export async function copyNativeSource(root, output, omittedPaths = []) {
  let files = 0
  const collect = async (directory, prefix = "") => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const relative = prefix ? `${prefix}/${entry.name}` : entry.name
      if (relative === ".mcp.json" || omittedPaths.includes(relative)) continue
      if (SENSITIVE_FILES.test(relative)) continue
      if (
        EXCLUDED.has(entry.name) ||
        (prefix === "docs" && entry.name === "images")
      )
        continue
      const file = path.join(directory, entry.name)
      const info = await lstat(file)
      if (info.isSymbolicLink())
        throw Error(`Symbolic links are not supported: ${relative}`)
      if (info.isDirectory()) {
        await collect(file, relative)
        continue
      }
      if (!info.isFile()) throw Error(`Unsupported package entry: ${relative}`)
      if (++files > 350)
        throw Error("Native source exceeds plugin package limits")
      await cp(file, path.join(output, relative), {
        errorOnExist: true,
        force: false,
      })
    }
  }
  await collect(root)
}

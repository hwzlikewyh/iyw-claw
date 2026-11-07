import { writeFile } from "node:fs/promises"
import path from "node:path"
import { writeJson } from "./json.mjs"
import { toolVisibility } from "./capture.mjs"
import { environmentNames } from "./environment.mjs"
const KEY = /^[a-z0-9]+(?:-[a-z0-9]+)*$/
const normalizedKey = (value) =>
  value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")

export async function addTools(state, connector) {
  for (const tool of connector.tools) {
    const key = `${connector.key}-${normalizedKey(tool.name)}`
    if (
      !KEY.test(key) ||
      state.keys.has(key) ||
      tool.inputSchema?.type !== "object"
    )
      throw Error(`Invalid or duplicate tool: ${tool.name}`)
    state.keys.add(key)
    const schemaPath = `contracts/${key}.schema.json`
    await writeJson(path.join(state.output, schemaPath), tool.inputSchema)
    state.components.capabilities.push({
      key,
      id: `plugin.${state.name}.${connector.key}.${normalizedKey(tool.name)}.v1`,
      connectorKey: connector.key,
      toolName: tool.name,
      schemaPath,
      description: tool.description || tool.title || tool.name,
      intentTerms: [state.name, tool.name, ...(tool.title ? [tool.title] : [])],
      readOnlyHint: tool.annotations?.readOnlyHint === true,
    })
    const uri = tool._meta?.ui?.resourceUri ?? tool._meta?.["ui/resourceUri"]
    if (!uri) continue
    if (!connector.resources.some((resource) => resource.uri === uri))
      throw Error(`Missing tool UI resource: ${uri}`)
    const nativeModes = connector.metadata[uri]?.["openai/ui"]
      ?.availableDisplayModes ?? ["inline", "fullscreen"]
    const modes = nativeModes.filter((mode) =>
      ["inline", "fullscreen"].includes(mode)
    )
    if (!modes.length) throw Error(`Unsupported UI display modes: ${uri}`)
    state.components.apps.push({
      key: `app-${key}`,
      connectorKey: connector.key,
      resourceUri: uri,
      capabilityKey: key,
      displayModes: modes,
    })
  }
}

export async function addRuntime(state, connector) {
  const entrypoint = `runtime/${connector.key}.mjs`
  const settings = {
    sourceRoot: "../source",
    entrypoint: connector.entrypoint,
    cwd: connector.spec.cwd,
    env: connector.spec.env ?? {},
  }
  await writeJson(
    path.join(state.output, `runtime/${connector.key}.json`),
    settings
  )
  await writeFile(
    path.join(state.output, entrypoint),
    `import { startNativeRuntime } from "./native-runtime.mjs"\nawait startNativeRuntime(new URL("./${connector.key}.json", import.meta.url))\n`,
    { flag: "wx" }
  )
  state.components.runtimes.push({
    key: connector.key,
    kind: "node",
    entrypoint,
    dependencies: "bundled",
  })
  state.components.connectors.push({
    key: connector.key,
    transport: "stdio",
    runtimeKey: connector.key,
    routing: { mode: "host_gateway" },
    activation: { mode: "lazy", scope: "workspace" },
  })
  state.native.connectors[connector.key] = nativeConnector(
    connector,
    entrypoint
  )
}

function nativeConnector(connector, entrypoint) {
  const envVars = environmentNames(connector.spec)
  return {
    entrypoint,
    envVars,
    dataDirEnvVars: envVars.filter((name) => /_DATA_DIR$/i.test(name)),
    startupTimeoutSec: connector.spec.startup_timeout_sec ?? 180,
    toolTimeoutSec: connector.spec.tool_timeout_sec ?? 120,
    modelTools: connector.tools
      .filter((tool) => toolVisibility(tool).includes("model"))
      .map((tool) => tool.name),
    appTools: connector.tools
      .filter((tool) => toolVisibility(tool).includes("app"))
      .map((tool) => tool.name),
    resources: connector.resources.filter((item) =>
      item.uri.startsWith("ui://")
    ),
    resourceTemplates: connector.resourceTemplates,
    wasmUnsafeEval: Object.values(connector.metadata).some((meta) => meta.ui?.csp?.wasmUnsafeEval === true),
    preferredDisplayModes: Object.fromEntries(
      Object.entries(connector.metadata).map(([uri, meta]) => [
        uri,
        meta["openai/ui"]?.preferredDisplayMode ?? "inline",
      ])
    ),
  }
}

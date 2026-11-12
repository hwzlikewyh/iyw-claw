import path from "node:path"
import { PluginRpc } from "./rpc.mjs"
import { resolveServer } from "./source.mjs"
import { nativeEnvironment } from "./runtime.mjs"
import { readResourceTemplates } from "./resources.mjs"
import { environmentNames } from "./environment.mjs"
const KEY = /^[a-z0-9]+(?:-[a-z0-9]+)*$/

export function collectPermissions(state, metadata) {
  for (const meta of Object.values(metadata)) {
    for (const key of [
      "connectDomains",
      "resourceDomains",
      "frameDomains",
      "baseUriDomains",
    ]) {
      for (const domain of meta.ui?.csp?.[key] ?? []) {
        if (
          ["data:", "blob:"].includes(domain) &&
          ["connectDomains", "resourceDomains"].includes(key)
        )
          continue
        const url = new URL(domain)
        if (
          url.protocol !== "https:" ||
          url.username ||
          url.password ||
          url.search ||
          url.hash ||
          url.pathname !== "/"
        )
          throw Error(`Unsupported UI CSP origin: ${domain}`)
        if (key === "baseUriDomains")
          throw Error(
            "Native base URI permissions require explicit package support"
          )
        const origins = state.permissions.network[key]
        if (!origins.includes(domain)) origins.push(domain)
      }
    }
    for (const [key, permission] of [
      ["camera", "camera"],
      ["microphone", "microphone"],
      ["geolocation", "geolocation"],
      ["clipboardWrite", "clipboard-write"],
    ]) {
      if (
        meta.ui?.permissions?.[key] &&
        !state.permissions.host.includes(permission)
      )
        state.permissions.host.push(permission)
    }
  }
}

export async function captureConnector(source, key, spec) {
  if (!KEY.test(key)) throw Error(`Invalid connector key: ${key}`)
  environmentNames(spec)
  validateEnvironmentTemplates(spec.env)
  const entrypoint = await resolveServer(source, spec)
  if (
    ![spec.startup_timeout_sec ?? 180].every(
      (value) => Number.isInteger(value) && value > 0 && value <= 600
    ) ||
    ![spec.tool_timeout_sec ?? 120].every(
      (value) => Number.isInteger(value) && value > 0 && value <= 7200
    )
  )
    throw Error("Native MCP timeouts exceed supported limits")
  const cwd = spec.cwd ?? "."
  if (
    path.isAbsolute(cwd) ||
    path.relative(source.root, path.resolve(source.root, cwd)).startsWith("..")
  )
    throw Error("Native cwd must stay inside the plugin")
  const env = nativeEnvironment(spec.env, source.root)
  const client = new PluginRpc({
    entrypoint,
    cwd: path.resolve(source.root, cwd),
    env,
    timeout: (spec.startup_timeout_sec ?? 180) * 1000,
  })
  try {
    const info = await client.initialize()
    const tools = await client.list("tools/list", "tools")
    const resources = info.capabilities?.resources
      ? await client.list("resources/list", "resources")
      : []
    const metadata = await readUiMetadata(client, resources)
    const resourceTemplates = await readResourceTemplates(client, !!info.capabilities?.resources)
    return {
      key,
      spec,
      tools,
      resources,
      metadata,
      resourceTemplates,
      entrypoint: path.relative(source.root, entrypoint).replaceAll("\\", "/"),
    }
  } finally {
    await client.close()
  }
}

function validateEnvironmentTemplates(values = {}) {
  for (const [key, value] of Object.entries(values)) {
    if (typeof value !== "string")
      throw Error("Native environment values must be strings")
    if (isAbsoluteEnvironmentPath(value)) {
      throw Error(
        `Environment path ${key} must use a host variable or plugin root`
      )
    }
    if (
      /(?:SECRET|TOKEN|PASSWORD|API[_-]?KEY|COOKIE|CREDENTIAL|AUTH)/i.test(
        key
      ) &&
      !/^\$\{[A-Za-z_][A-Za-z0-9_]*(?::-[^{}]*)?\}$/.test(value)
    )
      throw Error(`Secret ${key} must reference a host environment variable`)
  }
}

function isAbsoluteEnvironmentPath(value) {
  return (
    path.isAbsolute(value) ||
    /^[A-Za-z]:[\\/]/.test(value) ||
    value.startsWith("\\\\")
  )
}

async function readUiMetadata(client, resources) {
  const metadata = {}
  for (const resource of resources.filter((item) =>
    item.uri.startsWith("ui://")
  )) {
    const result = await client.call("resources/read", { uri: resource.uri })
    metadata[resource.uri] =
      result.contents?.find((item) => item.uri === resource.uri)?._meta ??
      resource._meta ??
      {}
  }
  return metadata
}

export function toolVisibility(tool) {
  const visibility = tool._meta?.ui?.visibility ?? ["model", "app"]
  if (
    !Array.isArray(visibility) ||
    visibility.some((value) => !["model", "app"].includes(value))
  )
    throw Error(`Invalid tool visibility: ${tool.name}`)
  return visibility
}

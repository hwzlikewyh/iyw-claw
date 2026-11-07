import { readFile, writeFile } from "node:fs/promises"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { writeJson } from "./json.mjs"
import { captureConnector, collectPermissions } from "./capture.mjs"
import { addRuntime, addTools } from "./components.mjs"
import { addSkills } from "./skills.mjs"
import { validatePreparation } from "../validate_plugin_preparation.mjs"
import { minimumClientVersion } from "./client-version.mjs"

function createState(source, output) {
  return {
    output,
    name: source.manifest.name,
    keys: new Set(),
    components: {
      skills: [],
      runtimes: [],
      connectors: [],
      capabilities: [],
      apps: [],
    },
    native: {
      schemaVersion: 1,
      source: { name: source.manifest.name, version: source.manifest.version },
      connectors: {},
    },
    permissions: {
      workspace: { read: [], write: [] },
      network: { connectDomains: [], resourceDomains: [], frameDomains: [] },
      host: ["send-message", "open-link"],
    },
  }
}

export async function buildManifest(source, output, options = {}) {
  const minimum = minimumClientVersion(options.minClientVersion)
  const state = createState(source, output)
  for (const [key, spec] of Object.entries(source.servers)) {
    const connector = await captureConnector(source, key, spec)
    collectPermissions(state, connector.metadata)
    await addRuntime(state, connector)
    await addTools(state, connector)
  }
  await addSkills(state, source)
  const runtime = await readFile(
    fileURLToPath(new URL("./runtime.mjs", import.meta.url))
  )
  await writeFile(path.join(output, "runtime/native-runtime.mjs"), runtime, {
    flag: "wx",
  })
  await writeJson(path.join(output, "native-plugin.json"), state.native)
  const manifest={
    schemaVersion: 2,
    name: state.name,
    version: source.manifest.version,
    targets: ["iyw-claw"],
    components: state.components,
    permissions: state.permissions,
    ...(minimum ? { minClientVersion: minimum } : {}),
  }
  validatePreparation(manifest)
  await writeJson(path.join(output, ".iyw-plugin.json"), manifest)
  return {
    name: state.name,
    version: source.manifest.version,
    components: Object.fromEntries(
      Object.entries(state.components).map(([kind, values]) => [
        kind,
        values.length,
      ])
    ),
  }
}

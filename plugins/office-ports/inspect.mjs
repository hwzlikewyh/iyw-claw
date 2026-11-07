import { readFile, writeFile } from "node:fs/promises"
import path from "node:path"
import { createHash } from "node:crypto"
import { PluginRpc } from "../../src-tauri/experts/skills/plugin-creator/scripts/native-plugin/rpc.mjs"

const readJson = async (file) => JSON.parse(await readFile(file, "utf8"))

export async function inspectPackage(root) {
  const manifest = await readJson(path.join(root, ".iyw-plugin.json"))
  const runtime = manifest.components.runtimes[0]
  const client = new PluginRpc({
    entrypoint: path.join(root, runtime.entrypoint),
    cwd: root,
    env: {},
    timeout: 30_000,
  })
  try {
    const info = await client.initialize()
    const tools = await client.list("tools/list", "tools")
    await inspectTools(root, manifest, tools)
    const resources = await client.list("resources/list", "resources")
    const resourceTemplates = await client.list(
      "resources/templates/list",
      "resourceTemplates"
    )
    const assets = []
    for (const resource of resources) {
      assets.push(await inspectResource(client, root, resource.uri))
      const prefix =
        resource.uri.slice(0, resource.uri.lastIndexOf("/") + 1) + "assets/"
      assets.push(await inspectResource(client, root, prefix + "assets.json"))
      const index = await client.call("resources/read", {
        uri: prefix + "assets.json",
      })
      for (const name of JSON.parse(index.contents[0].text).scripts)
        assets.push(await inspectResource(client, root, prefix + name))
      assets.push(await inspectResource(client, root, prefix + "panel.css"))
    }
    const proof = {
      server: info.serverInfo,
      tools: tools.length,
      resources: resources.map((value) => value.uri),
      resourceTemplates,
      assets,
      businessToolsCalled: false,
    }
    await writeFile(
      path.join(root, "port-inspection.json"),
      `${JSON.stringify(proof, null, 2)}\n`
    )
    return proof
  } finally {
    await client.close()
  }
}

async function inspectTools(root, manifest, tools) {
  if (tools.length !== manifest.components.capabilities.length)
    throw Error("Packaged tool inventory changed")
  for (const capability of manifest.components.capabilities) {
    const actual = tools.find((tool) => tool.name === capability.toolName)
    const expected = await readJson(path.join(root, capability.schemaPath))
    if (
      !actual ||
      JSON.stringify(actual.inputSchema) !== JSON.stringify(expected)
    )
      throw Error(`Packaged tool contract changed: ${capability.toolName}`)
  }
}

async function inspectResource(client, root, uri) {
  const result = await client.call("resources/read", { uri })
  const content = result.contents?.find((value) => value.uri === uri)
  if (!content || typeof content.text !== "string")
    throw Error("Expected packaged text resource")
  const name = uri.endsWith("/panel.html")
    ? "panel.html"
    : uri.slice(uri.indexOf("/assets/") + "/assets/".length)
  if (!name || name.includes("..") || name.includes("\\"))
    throw Error("Unsafe packaged asset")
  const expected = await readFile(path.join(root, "source/dist/ui", name))
  const actual = Buffer.from(content.text)
  if (!expected.equals(actual)) throw Error(`Packaged asset differs: ${name}`)
  return {
    uri,
    bytes: actual.length,
    sha256: createHash("sha256").update(actual).digest("hex"),
  }
}

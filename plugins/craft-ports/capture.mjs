import { mkdir, readFile, realpath, writeFile } from "node:fs/promises"
import path from "node:path"
import { createHash } from "node:crypto"
import {
  readNativeSource,
  copyNativeSource,
} from "../../src-tauri/experts/skills/plugin-creator/scripts/native-plugin/source.mjs"
import { buildManifest } from "../../src-tauri/experts/skills/plugin-creator/scripts/native-plugin/manifest.mjs"
import { packageStats } from "../../src-tauri/experts/skills/plugin-creator/scripts/native-plugin/package.mjs"
import { PluginRpc } from "../../src-tauri/experts/skills/plugin-creator/scripts/native-plugin/rpc.mjs"

await capture().catch((error) => {
  console.log(JSON.stringify({ error: error.message }))
  process.exitCode = 1
})

async function capture() {
  const stage = await realpath(process.argv[2]),
    source = await readNativeSource(path.join(stage, "author")),
    output = path.join(stage, "package")
  await mkdir(output)
  await copyNativeSource(source.root, path.join(output, "source"), [
    source.configPath,
  ])
  const manifest = await buildManifest(
    { ...source, root: await realpath(path.join(output, "source")) },
    output,
    { minClientVersion: process.argv[3] }
  )
  const packageManifestPath = path.join(output, ".iyw-plugin.json"),
    value = JSON.parse(await readFile(packageManifestPath, "utf8"))
  value.permissions.workspace = { read: ["**"], write: ["**"] }
  value.platforms = [
    "windows-x86_64",
    "windows-aarch64",
    "macos-x86_64",
    "macos-aarch64",
    "linux-x86_64",
    "linux-aarch64",
  ]
  await writeFile(packageManifestPath, `${JSON.stringify(value, null, 2)}\n`)
  const inspection = await inspect(output, value)
  await writeFile(
    path.join(output, "port-inspection.json"),
    `${JSON.stringify(inspection, null, 2)}\n`
  )
  console.log(
    JSON.stringify({ ...manifest, ...(await packageStats(output)), inspection })
  )
}

async function inspect(root, manifest) {
  const client = new PluginRpc({
    entrypoint: path.join(root, manifest.components.runtimes[0].entrypoint),
    cwd: root,
    env: {},
    timeout: 30_000,
  })
  try {
    const info = await client.initialize(),
      tools = await client.list("tools/list", "tools")
    const resources = await client.list("resources/list", "resources"),
      templates = await client.list(
        "resources/templates/list",
        "resourceTemplates"
      )
    for (const capability of manifest.components.capabilities) {
      const expected = JSON.parse(
        await readFile(path.join(root, capability.schemaPath), "utf8")
      )
      const tool = tools.find((tool) => tool.name === capability.toolName)
      if (
        !tool ||
        JSON.stringify(tool.inputSchema) !== JSON.stringify(expected)
      )
        throw Error("Packaged editor tool contract changed")
    }
    const config = JSON.parse(
      (
        await client.call("resources/read", {
          uri: resources.find((resource) =>
            resource.uri.endsWith("assets.json")
          ).uri,
        })
      ).contents[0].text
    )
    const assets = await inspectAssets(client, config)
    return {
      server: info.serverInfo,
      tools: tools.length,
      resources,
      templates,
      assets,
      businessToolsCalled: false,
    }
  } finally {
    await client.close()
  }
}

async function inspectAssets(client, config) {
  const assets = []
  for (const entry of config.assets) {
    const chunks = []
    for (let part = 0; part < entry.parts; part++) {
      const result = await client.call("resources/read", {
        uri: `ui://${config.name}/asset/${entry.id}/${part}`,
      })
      chunks.push(Buffer.from(result.contents[0].blob, "base64"))
    }
    const bytes = Buffer.concat(chunks),
      sha256 = createHash("sha256").update(bytes).digest("hex")
    if (bytes.length !== entry.bytes || sha256 !== entry.sha256)
      throw Error("Packaged editor asset did not reassemble correctly")
    assets.push({ path: entry.path, bytes: bytes.length, sha256 })
  }
  return assets
}

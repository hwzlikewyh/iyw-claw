import { build } from "esbuild"
import { copyFile, mkdir, readFile, readdir, writeFile } from "node:fs/promises"
import { createHash } from "node:crypto"
import { dirname, join, resolve, sep } from "node:path"
import { fileURLToPath } from "node:url"

const root = dirname(fileURLToPath(import.meta.url))
const output = join(root, "dist")
const resourceRoots = {
  assets: process.env.IYW_CONNECTOR_ASSET_ROOT,
  workbuddy: process.env.IYW_CONNECTOR_SOURCE_ROOT,
}
const bundle = await build({
  absWorkingDir: root,
  entryPoints: ["main.mjs"],
  write: false,
  bundle: true,
  preserveSymlinks: true,
  platform: "node",
  format: "esm",
  target: "node22",
  banner: {
    js: "import {createRequire} from 'node:module';const require=createRequire(import.meta.url);",
  },
})

function resourceSource(source) {
  const [kind, ...parts] = source.split("/")
  const base = resourceRoots[kind]
  if (!base)
    throw new Error(`缺少 ${kind} 构建资源目录，请按 README 设置环境变量`)
  const path = resolve(base, ...parts)
  if (!path.startsWith(resolve(base) + sep)) throw new Error("构建资源路径越界")
  return path
}

async function copyResources(recipe, destination) {
  for (const item of recipe.resources ?? []) {
    const target = resolve(destination, item.path)
    if (!target.startsWith(resolve(destination) + sep))
      throw new Error("制品文件路径越界")
    const source = resourceSource(item.source)
    const bytes = await readFile(source)
    const digest = createHash("sha256").update(bytes).digest("hex")
    if (digest !== item.sha256)
      throw new Error(`${recipe.id} 的供应商源码已变化，请重新审核导入`)
    await mkdir(dirname(target), { recursive: true })
    await copyFile(source, target)
  }
}

function installOption(recipe) {
  const entry = "${ARTIFACT_DIR}/adapter.mjs"
  const cli = (mode) => ({ command: "node", args: [entry, mode] })
  const option = {
    id: `${recipe.id}-windows-x64`,
    label: `${recipe.name}（Windows x64）`,
    protocol: "stdio",
    platforms: ["windows-x86_64"],
    runtime: "node",
    auth_mode: "cli",
    description: "首次登录自动准备独立运行环境，再按服务方要求完成授权。",
    parameters: recipe.parameters ?? [],
    spec: {
      type: "stdio",
      ...cli("serve"),
      env: {
        IYW_CONNECTOR_AUTH_DIR: "${CONNECTOR_AUTH_DIR}",
        ...Object.fromEntries(
          (recipe.parameters ?? []).map((field) => [
            field.key,
            `\${${field.key}}`,
          ])
        ),
      },
    },
    cli_auth: { login: cli("login"), status: cli("status") },
  }
  if (recipe.logout?.length || recipe.clearAuthDirectories?.length)
    option.cli_auth.logout = cli("logout")
  return option
}

const report = []
for (const filename of (await readdir(join(root, "recipes"))).filter((name) =>
  name.endsWith(".json")
)) {
  const recipe = JSON.parse(
    await readFile(join(root, "recipes", filename), "utf8")
  )
  const destination = join(output, recipe.id)
  await mkdir(destination, { recursive: true })
  await copyResources(recipe, destination)
  const manifest = { ...recipe }
  delete manifest.resources
  await writeFile(
    join(destination, "adapter.mjs"),
    bundle.outputFiles[0].contents
  )
  await writeFile(
    join(destination, "connector.json"),
    JSON.stringify(manifest, null, 2)
  )
  await writeFile(
    join(destination, "install-option.json"),
    JSON.stringify(installOption(recipe), null, 2)
  )
  report.push({
    id: recipe.id,
    type: recipe.mcp ? "mcp" : "cli",
    loginSteps: recipe.login.length,
  })
}
await writeFile(
  join(output, "build-report.json"),
  JSON.stringify(report, null, 2)
)
console.log(JSON.stringify({ built: report.length, output }))

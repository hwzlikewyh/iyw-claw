import { readFile, mkdir } from "node:fs/promises"
import { dirname, delimiter, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { createHash } from "node:crypto"

const REVISION_LENGTH = 16

function childEnvironment({ account, runtime, manifest, expand }) {
  const env = { ...process.env }
  // 不向供应商安装器传递宿主智能体目录，避免覆盖内置技能。
  for (const key of [
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "HERMES_HOME",
    "OPENCLAW_HOME",
    "GROK_HOME",
    "AUTOHAND_HOME",
    "VIBE_HOME",
  ])
    delete env[key]
  env.WORKBUDDY_CONFIG_DIR = join(account, ".workbuddy")
  env.CODEBUDDY_CONFIG_DIR = join(account, ".codebuddy")
  for (const [key, value] of Object.entries(manifest.env ?? {}))
    env[key] = expand(value)
  const pathKey =
    Object.keys(env).find((key) => key.toLowerCase() === "path") ?? "PATH"
  env[pathKey] = [
    join(runtime, "npm/node_modules/.bin"),
    join(runtime, "python/Scripts"),
    join(runtime, "bin"),
    dirname(process.execPath),
    env[pathKey] ?? "",
  ].join(delimiter)
  env.npm_config_prefix = join(runtime, "npm")
  env.npm_config_cache = join(runtime, "npm-cache")
  env.PIP_DISABLE_PIP_VERSION_CHECK = "1"
  return env
}

export async function loadContext() {
  const artifact = dirname(fileURLToPath(import.meta.url))
  const manifest = JSON.parse(
    await readFile(join(artifact, "connector.json"), "utf8")
  )
  const account = process.env.IYW_CONNECTOR_AUTH_DIR
  if (!account) throw new Error("缺少连接器账号目录，请从连接器登录界面启动")
  if (manifest.blockers?.length) throw new Error(manifest.blockers.join("；"))
  const revision = createHash("sha256")
    .update(JSON.stringify(manifest))
    .digest("hex")
    .slice(0, REVISION_LENGTH)
  const runtime = join(resolve(account), ".runtime", revision)
  const paths = {
    AUTH_DIR: resolve(account),
    ARTIFACT_DIR: artifact,
    RUNTIME_DIR: runtime,
    ACCOUNT_ID: createHash("sha256").update(resolve(account)).digest("hex"),
  }
  const expand = (text) =>
    text.replace(
      /\{\{(AUTH_DIR|ARTIFACT_DIR|RUNTIME_DIR|ACCOUNT_ID)\}\}/g,
      (_, name) => paths[name]
    )
  const env = childEnvironment({
    account: resolve(account),
    runtime,
    manifest,
    expand,
  })
  await mkdir(runtime, { recursive: true })
  return { manifest, artifact, account: resolve(account), runtime, env, expand }
}

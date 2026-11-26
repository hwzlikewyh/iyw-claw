import { access, mkdir, readFile, writeFile } from "node:fs/promises"
import { dirname, isAbsolute, join, resolve, sep } from "node:path"
import { createHash } from "node:crypto"
import { execute } from "./process.mjs"
import { applyDefaults } from "./defaults.mjs"
import { verifyVersion } from "./version.mjs"
import { prepareNativeFiles } from "./native.mjs"

const INSTALL_TIMEOUT_MS = 240_000
const READY_FILE = "installed.json"

async function present(path) {
  try {
    await access(path)
    return true
  } catch {
    return false
  }
}

function identity(context) {
  return createHash("sha256")
    .update(JSON.stringify(context.manifest))
    .digest("hex")
}

async function resolveEntries(context) {
  const manifest = context.manifest
  const commands = [
    manifest.status,
    ...manifest.tools.map((t) => t.command),
    ...manifest.login.flatMap((step) =>
      [step.command, step.skipIf].filter(Boolean)
    ),
    ...(manifest.logout ?? []),
  ]
  if (manifest.mcp) commands.push(manifest.mcp)
  for (const command of commands) await resolveCommand(context, command)
}

export async function ready(context) {
  try {
    const saved = JSON.parse(
      await readFile(join(context.runtime, READY_FILE), "utf8")
    )
    if (saved.identity !== identity(context)) return false
    await resolveEntries(context)
    return true
  } catch {
    return false
  }
}

async function npmInstall(context, packages) {
  const npm = join(dirname(process.execPath), "node_modules/npm/bin/npm-cli.js")
  await access(npm)
  const folder = join(context.runtime, "npm")
  await mkdir(folder, { recursive: true })
  await writeFile(
    join(folder, "package.json"),
    JSON.stringify({
      private: true,
      allowScripts: Object.fromEntries(
        packages.map((p) => [p.integrity ? p.spec : p.name, true])
      ),
    })
  )
  const result = await execute(
    context,
    {
      program: process.execPath,
      args: [
        npm,
        "install",
        "--prefix",
        folder,
        "--no-audit",
        "--no-fund",
        "--save-exact",
        ...packages.map((p) => context.expand(p.spec)),
      ],
    },
    { inherit: true, timeoutMs: INSTALL_TIMEOUT_MS }
  )
  if (result.code !== 0)
    throw new Error(`连接器 npm 依赖安装失败（退出码 ${result.code}）`)
}

async function pythonInstall(context, packages) {
  const folder = join(context.runtime, "python")
  if (!(await present(join(folder, "Scripts/python.exe")))) {
    const result = await execute(
      context,
      { program: "uv", args: ["venv", "--python", "3.12", "--seed", folder] },
      { inherit: true, timeoutMs: INSTALL_TIMEOUT_MS }
    )
    if (result.code !== 0) throw new Error("无法创建连接器 Python 独立环境")
  }
  const result = await execute(
    context,
    {
      program: join(folder, "Scripts/python.exe"),
      args: ["-m", "pip", "install", ...packages],
    },
    { inherit: true, timeoutMs: INSTALL_TIMEOUT_MS }
  )
  if (result.code !== 0)
    throw new Error(`连接器 Python 依赖安装失败（退出码 ${result.code}）`)
}

export async function prepare(context) {
  if (await ready(context)) return
  console.error(`正在准备 ${context.manifest.name} 的独立运行环境…`)
  await prepareNativeFiles(context)
  if (context.manifest.packages?.length)
    await npmInstall(context, context.manifest.packages)
  if (context.manifest.python?.length)
    await pythonInstall(context, context.manifest.python)
  for (const step of context.manifest.install ?? []) {
    const result = await execute(context, await resolveCommand(context, step), {
      inherit: true,
      timeoutMs: INSTALL_TIMEOUT_MS,
    })
    if (result.code !== 0)
      throw new Error(`连接器安装步骤失败（退出码 ${result.code}）`)
  }
  await resolveEntries(context)
  if (context.manifest.versionCheck) {
    await verifyVersion(
      context,
      await resolveCommand(context, context.manifest.versionCheck.command)
    )
  }
  await applyDefaults(context)
  await writeFile(
    join(context.runtime, READY_FILE),
    JSON.stringify({ identity: identity(context) })
  )
}

async function npmBinary(context, name, packageName) {
  for (const dependency of context.manifest.packages ?? []) {
    if (packageName && dependency.name !== packageName) continue
    const root = join(context.runtime, "npm/node_modules", dependency.name)
    const metadata = JSON.parse(
      await readFile(join(root, "package.json"), "utf8")
    )
    const bins =
      typeof metadata.bin === "string"
        ? { [metadata.name.split("/").pop()]: metadata.bin }
        : (metadata.bin ?? {})
    if (!bins[name]) continue
    const path = resolve(root, bins[name])
    if (!path.startsWith(resolve(root) + sep))
      throw new Error("CLI 入口超出依赖目录")
    await access(path)
    return path.endsWith(".exe")
      ? { program: path, args: [] }
      : { program: process.execPath, args: [path] }
  }
  return null
}

export async function resolveCommand(context, command) {
  const args = (command.args ?? []).map(context.expand)
  const name = context.expand(command.program)
  if (name === "node") return { program: process.execPath, args }
  if (name === "python") {
    const python = join(context.runtime, "python/Scripts/python.exe")
    await access(python)
    return { program: python, args }
  }
  if (isAbsolute(name)) {
    await access(name)
    return { program: name, args }
  }
  const bin = await npmBinary(
    context,
    name.replace(/\.(cmd|exe)$/i, ""),
    command.package
  )
  if (bin) return { program: bin.program, args: [...bin.args, ...args] }
  const pythonBin = join(
    context.runtime,
    "python/Scripts",
    `${name.replace(/\.exe$/i, "")}.exe`
  )
  if (await present(pythonBin)) return { program: pythonBin, args }
  throw new Error(`连接器程序 ${name} 不存在，请重新运行登录以修复运行环境`)
}

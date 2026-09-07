import { execFile } from "node:child_process"
import { mkdir, mkdtemp, readFile, rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import { basename, delimiter, dirname, join } from "node:path"
import { promisify } from "node:util"
import { targetInfo } from "./runtime-seed-config.mjs"
import { archiveTar, safeRelativePath } from "./runtime-seed-files.mjs"

const execFileAsync = promisify(execFile)
const PROBE_TIMEOUT_MS = 15_000
const PROBE_OUTPUT_LIMIT = 1024 * 1024
const COMPONENT_IDS = ["node", "git", "uv", "codex-acp"]

function canExecuteTarget(target) {
  const info = targetInfo(target)
  if (info.skipped) return false
  if (
    info.os === "linux" &&
    info.npm.join("-") !== `${process.platform}-${process.arch}`
  ) {
    console.log(
      `[runtime-seed] launch probe skipped for cross-compiled target ${target}`
    )
    return false
  }
  if (info.npm[0] !== process.platform)
    throw new Error(
      `Runtime seed launch verification for ${target} requires ${info.npm[0]}`
    )
  // Mac x64 在 ARM runner 上直接执行目标二进制；缺少 Rosetta 时失败，不能跳过。
  return true
}

async function extractComponents(seedRoot, components, destination) {
  const staged = {}
  for (const id of COMPONENT_IDS) {
    const component = components.find((item) => item.id === id)
    if (!component || !safeRelativePath(component.archive ?? ""))
      throw new Error(
        `Runtime seed launch probe component is missing or invalid: ${id}`
      )
    const archive = join(seedRoot, component.archive)
    const root = join(destination, id)
    await mkdir(root)
    await archiveTar(archive, ["-xf", basename(archive), "-C", root])
    staged[id] = { ...component, root }
  }
  return staged
}

function executable(component, name) {
  const entry = component.entrypoints?.[name]
  if (!safeRelativePath(entry ?? ""))
    throw new Error(`Runtime seed launch probe entrypoint is invalid: ${name}`)
  return join(component.root, entry)
}

async function probe({ label, command, args = ["--version"], env, expected }) {
  let result
  try {
    result = await execFileAsync(command, args, {
      env,
      windowsHide: true,
      timeout: PROBE_TIMEOUT_MS,
      maxBuffer: PROBE_OUTPUT_LIMIT,
    })
  } catch (error) {
    throw new Error(`Runtime seed ${label} launch failed: ${error.message}`, {
      cause: error,
    })
  }
  const output = `${result.stdout}\n${result.stderr}`.trim()
  if (!output || (expected && !output.includes(expected.split("+")[0])))
    throw new Error(
      `Runtime seed ${label} returned an unexpected version (expected ${expected ?? "version output"})`
    )
  console.log(`[runtime-seed] ${label} launch verified`)
  return result.stdout.trim()
}

async function probeNode(component, env, target) {
  const command = executable(component, "node")
  await probe({ label: "Node.js", command, env, expected: component.version })
  const identity = await probe({
    label: "Node.js target",
    command,
    args: ["-p", "JSON.stringify([process.platform, process.arch])"],
    env,
  })
  if (identity !== JSON.stringify(targetInfo(target).npm))
    throw new Error(
      `Runtime seed Node.js architecture does not match ${target}`
    )
  for (const name of ["npm", "npx"]) {
    const windows = targetInfo(target).os === "windows"
    await probe({
      label: name,
      command: windows ? command : join(component.root, "bin", name),
      args: windows
        ? [
            join(component.root, "node_modules/npm/bin", `${name}-cli.js`),
            "--version",
          ]
        : ["--version"],
      env,
    })
  }
}

async function probeCodex(component, { node, env, target }) {
  const packageRoot = join(
    component.root,
    "node_modules/@agentclientprotocol/codex-acp"
  )
  const manifest = JSON.parse(
    await readFile(join(packageRoot, "package.json"), "utf8")
  )
  const entry = manifest.bin?.["codex-acp"]
  if (!safeRelativePath(entry ?? ""))
    throw new Error("Codex launch probe entrypoint is invalid")
  await probe({
    label: "Codex ACP",
    command: node,
    args: [join(packageRoot, entry), "--version"],
    env,
    expected: component.version,
  })
  if (targetInfo(target).os !== "windows")
    await probe({
      label: "Codex ACP launcher",
      command: executable(component, "codex-acp"),
      env,
      expected: component.version,
    })
}

export async function verifyRuntimeSeedLaunch(seedRoot, components, target) {
  if (!canExecuteTarget(target)) return
  const destination = await mkdtemp(join(tmpdir(), "iyw-seed-launch-"))
  try {
    const staged = await extractComponents(seedRoot, components, destination)
    const node = executable(staged.node, "node")
    const env = {
      ...process.env,
      PATH: [dirname(node), process.env.PATH ?? ""].join(delimiter),
      npm_config_cache: join(destination, "npm-cache"),
      npm_config_userconfig: join(destination, "npmrc"),
    }
    await probeNode(staged.node, env, target)
    await probe({
      label: "Git",
      command: executable(staged.git, "git"),
      env,
      expected: staged.git.version,
    })
    await probe({
      label: "uv",
      command: executable(staged.uv, "uv"),
      env,
      expected: staged.uv.version,
    })
    await probe({ label: "uvx", command: executable(staged.uv, "uvx"), env })
    await probeCodex(staged["codex-acp"], { node, env, target })
  } finally {
    await rm(destination, { recursive: true, force: true })
  }
}

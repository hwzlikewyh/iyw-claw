import { spawn } from "node:child_process"
import { mkdir, mkdtemp, realpath, rename, rm } from "node:fs/promises"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { prepareFork } from "./fork.mjs"

const PLUGINS = {
  documents: { version: "0.2.1", title: "Word 文档" },
  pdf: { version: "0.2.2", title: "PDF 文档" },
  spreadsheets: { version: "0.2.1", title: "电子表格" },
  presentations: { version: "0.2.1", title: "演示文稿" },
}

function options(args) {
  const values = {}
  for (let index = 0; index < args.length; index += 2) {
    const key = args[index]
    if (
      !["--cache", "--output", "--plugin", "--min-client-version"].includes(
        key
      ) ||
      !args[index + 1] ||
      values[key]
    )
      throw Error(
        "Use --cache <source-cache> --output <new-parent> [--plugin <name>]"
      )
    values[key] = args[index + 1]
  }
  if (
    !values["--cache"] ||
    !values["--output"] ||
    (values["--plugin"] && !PLUGINS[values["--plugin"]])
  )
    throw Error("Expected cache, output and a supported office plugin")
  return values
}

async function build({ cache, output, name, minimum }) {
  const config = PLUGINS[name]
  const target = path.join(output, `iyw-${name}`)
  const stage = await mkdtemp(path.join(output, ".office-port-"))
  try {
    const fork = path.join(stage, "author")
    await prepareFork({
      source: path.join(cache, name, config.version),
      fork,
      name,
      config,
    })
    const result = await capture(stage, minimum)
    await rename(path.join(stage, "package"), target)
    console.log(JSON.stringify({ ...result, output: target, published: false }))
  } finally {
    // 仅删除当前mkdtemp暂存；已有输出和源缓存不在清理范围。
    if (
      path.dirname(stage) === output &&
      path.basename(stage).startsWith(".office-port-")
    )
      await rm(stage, { recursive: true, force: true })
  }
}

async function capture(stage, minimum) {
  const profile = path.join(stage, "profile")
  const workspace = path.join(stage, "workspace")
  await mkdir(profile)
  await mkdir(workspace)
  const env = Object.fromEntries(
    ["SystemRoot", "windir", "ComSpec", "PATH", "Path", "PATHEXT"].flatMap(
      (key) => (process.env[key] ? [[key, process.env[key]]] : [])
    )
  )
  Object.assign(env, {
    HOME: profile,
    USERPROFILE: profile,
    TEMP: profile,
    TMP: profile,
    APPDATA: path.join(profile, "AppData/Roaming"),
    LOCALAPPDATA: path.join(profile, "AppData/Local"),
    IYW_WORKSPACE_DIR: workspace,
    IYW_PLUGIN_DATA_DIR: profile,
  })
  const child = spawn(
    process.execPath,
    [
      fileURLToPath(new URL("./capture.mjs", import.meta.url)),
      stage,
      ...(minimum ? [minimum] : []),
    ],
    { env, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] }
  )
  let output = ""
  child.stdout.on("data", (chunk) => {
    output += chunk
  })
  child.stderr.resume()
  await new Promise((resolve, reject) => {
    child.once("error", reject)
    child.once("exit", (code) =>
      code === 0
        ? resolve()
        : reject(Error(`Office package capture failed (${code})`))
    )
  })
  return JSON.parse(output)
}

const args = options(process.argv.slice(2))
const cache = await realpath(args["--cache"])
const output = path.resolve(args["--output"])
const relative = path.relative(cache, output)
if (!relative.startsWith("..") && !path.isAbsolute(relative))
  throw Error("Output must stay outside the source cache")
await mkdir(output, { recursive: true })
for (const name of args["--plugin"] ? [args["--plugin"]] : Object.keys(PLUGINS))
  await build({ cache, output, name, minimum: args["--min-client-version"] })

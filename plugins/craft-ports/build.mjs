import { spawn } from "node:child_process"
import { mkdir, mkdtemp, realpath, rename, rm } from "node:fs/promises"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { prepareCraft } from "./fork.mjs"
import { minimumClientVersion } from "../../src-tauri/experts/skills/plugin-creator/scripts/native-plugin/client-version.mjs"

const values = process.argv.slice(2)
const options = Object.fromEntries(
  values.reduce(
    (pairs, value, index) =>
      index % 2 ? pairs : [...pairs, [value, values[index + 1]]],
    []
  )
)
if (
  !options["--source"] ||
  !options["--output"] ||
  ![4, 6].includes(values.length) ||
  Object.keys(options).some(
    (key) => !["--source", "--output", "--min-client-version"].includes(key)
  )
)
  throw Error("Use --source <photocraft-0.2.4> --output <new-parent>")
const source = await realpath(options["--source"]),
  output = path.resolve(options["--output"])
minimumClientVersion(options["--min-client-version"])
const relative = path.relative(source, output)
if (!relative.startsWith("..") && !path.isAbsolute(relative))
  throw Error("Output must stay outside source cache")
await mkdir(output, { recursive: true })
const stage = await mkdtemp(path.join(output, ".craft-port-"))
try {
  await prepareCraft(source, path.join(stage, "author"))
  const result = await capture(stage, options["--min-client-version"])
  const target = path.join(output, "iyw-image-editor")
  await rename(path.join(stage, "package"), target)
  console.log(JSON.stringify({ ...result, output: target, published: false }))
} finally {
  if (
    path.dirname(stage) === output &&
    path.basename(stage).startsWith(".craft-port-")
  )
    await rm(stage, { recursive: true, force: true })
}

async function capture(stage, minimum) {
  const home = path.join(stage, "profile"),
    workspace = path.join(stage, "workspace")
  await mkdir(home)
  await mkdir(workspace)
  const env = Object.fromEntries(
    ["SystemRoot", "windir", "ComSpec", "PATH", "Path", "PATHEXT"].flatMap(
      (key) => (process.env[key] ? [[key, process.env[key]]] : [])
    )
  )
  Object.assign(env, {
    HOME: home,
    USERPROFILE: home,
    APPDATA: home,
    LOCALAPPDATA: home,
    XDG_CONFIG_HOME: home,
    XDG_CACHE_HOME: home,
    TEMP: home,
    TMP: home,
    IYW_WORKSPACE_DIR: workspace,
    IYW_PLUGIN_DATA_DIR: home,
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
  let stdout = ""
  child.stdout.on("data", (chunk) => {
    stdout += chunk
  })
  child.stderr.resume()
  await new Promise((resolve, reject) => {
    child.once("error", reject)
    child.once("exit", (code) =>
      code === 0
        ? resolve()
        : reject(
            Error(
              `Craft package capture failed (${code}): ${JSON.parse(stdout || "{}").error ?? "unknown cause"}`
            )
          )
    )
  })
  return JSON.parse(stdout)
}
